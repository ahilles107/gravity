//! Projects linked across peers, and the stand-ins a link keeps in step. See
//! "Linked projects" in `docs/peer-bots.md`.

use bus::*;
use rusqlite::{params, Row};

use super::{parse_ts, ts, Db};

const LINK_COLS: &str = "project_id, peer_id, remote_project_id, remote_project_name, linked_at";

fn link_from_row(r: &Row<'_>) -> rusqlite::Result<ProjectLink> {
    Ok(ProjectLink {
        project_id: r.get(0)?,
        peer_id: r.get(1)?,
        remote_project_id: r.get(2)?,
        remote_project_name: r.get(3)?,
        linked_at: parse_ts(&r.get::<_, String>(4)?),
    })
}

impl Db {
    /// Records a link. Fails when either project is already linked through
    /// this peer.
    pub fn create_project_link(
        &self,
        project_id: &str,
        peer_id: &str,
        remote_project_id: &str,
        remote_project_name: &str,
    ) -> anyhow::Result<ProjectLink> {
        let link = ProjectLink {
            project_id: project_id.to_string(),
            peer_id: peer_id.to_string(),
            remote_project_id: remote_project_id.to_string(),
            remote_project_name: remote_project_name.to_string(),
            linked_at: now(),
        };
        self.lock().execute(
            &format!("INSERT INTO project_link({LINK_COLS}) VALUES (?1, ?2, ?3, ?4, ?5)"),
            params![
                link.project_id,
                link.peer_id,
                link.remote_project_id,
                link.remote_project_name,
                ts(link.linked_at)
            ],
        )?;
        Ok(link)
    }

    pub fn delete_project_link(&self, project_id: &str, peer_id: &str) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "DELETE FROM project_link WHERE project_id = ?1 AND peer_id = ?2",
            params![project_id, peer_id],
        )?;
        Ok(changed > 0)
    }

    /// Keeps the other side's project name current. True when it changed.
    pub fn rename_remote_project(
        &self,
        project_id: &str,
        peer_id: &str,
        name: &str,
    ) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE project_link SET remote_project_name = ?3
             WHERE project_id = ?1 AND peer_id = ?2 AND remote_project_name != ?3",
            params![project_id, peer_id, name],
        )?;
        Ok(changed > 0)
    }

    fn query_links(&self, filter: &str, args: &[&str]) -> anyhow::Result<Vec<ProjectLink>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {LINK_COLS} FROM project_link WHERE {filter} ORDER BY linked_at"
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), link_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every link a project has, one per peer.
    pub fn project_links(&self, project_id: &str) -> anyhow::Result<Vec<ProjectLink>> {
        self.query_links("project_id = ?1", &[project_id])
    }

    /// Every link made through a peer.
    pub fn links_through(&self, peer_id: &str) -> anyhow::Result<Vec<ProjectLink>> {
        self.query_links("peer_id = ?1", &[peer_id])
    }

    pub fn project_link(
        &self,
        project_id: &str,
        peer_id: &str,
    ) -> anyhow::Result<Option<ProjectLink>> {
        Ok(self
            .query_links("project_id = ?1 AND peer_id = ?2", &[project_id, peer_id])?
            .pop())
    }

    /// The link a peer means when it names its own project.
    pub fn project_link_by_remote(
        &self,
        peer_id: &str,
        remote_project_id: &str,
    ) -> anyhow::Result<Option<ProjectLink>> {
        Ok(self
            .query_links(
                "peer_id = ?1 AND remote_project_id = ?2",
                &[peer_id, remote_project_id],
            )?
            .pop())
    }

    /// The live stand-ins a peer has in a project.
    pub fn stand_ins(&self, project_id: &str, peer_id: &str) -> anyhow::Result<Vec<Bot>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {} FROM bot WHERE project_id = ?1 AND peer_id = ?2 AND deleted_at IS NULL \
             ORDER BY created_at",
            Self::BOT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![project_id, peer_id], Self::bot_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Stops a peer delivering to the project's bots: the link that let it is
    /// gone.
    pub fn unexpose_project(&self, project_id: &str, peer_id: &str) -> anyhow::Result<()> {
        self.lock().execute(
            "DELETE FROM peer_link WHERE peer_id = ?1
             AND bot_id IN (SELECT id FROM bot WHERE project_id = ?2)",
            params![peer_id, project_id],
        )?;
        Ok(())
    }

    /// Makes `creator_id` the parent of a stand-in, which is what lets a bot
    /// manage a bot it created on the peer.
    pub fn set_bot_creator(&self, bot_id: &str, creator_id: &str) -> anyhow::Result<()> {
        self.lock().execute(
            "UPDATE bot SET created_by_bot_id = ?2 WHERE id = ?1",
            params![bot_id, creator_id],
        )?;
        Ok(())
    }

    /// Whether this project has a live bot by this name, other than `except`.
    pub fn bot_name_taken_except(
        &self,
        project_id: &str,
        name: &str,
        except: Option<&str>,
    ) -> anyhow::Result<bool> {
        Ok(self
            .get_bot_by_name(project_id, name)?
            .is_some_and(|b| Some(b.id.as_str()) != except))
    }
}
