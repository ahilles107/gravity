//! Project rows: creation, renaming, and archival deletion.
//!
//! A project is archived rather than deleted for the same reason a bot is:
//! `bot.project_id` and `conversation.project_id` are foreign keys into
//! `project`, so removing the row would orphan every bot and message it ever
//! held. See `MIGRATION_7`.

use bus::*;
use rusqlite::{params, OptionalExtension, Row};

use super::{parse_ts, ts, Db};

/// Marker separating an archived project's original name from its id suffix.
/// Outside the validated name charset, so a tombstone can never collide with a
/// live name.
const TOMBSTONE_SEP: char = '#';

impl Db {
    // ---- projects ----

    const PROJECT_COLS: &'static str = "id, name, dir_name, deleted_at, lead_bot_id, created_at";

    fn project_from_row(r: &Row<'_>) -> rusqlite::Result<Project> {
        Ok(Project {
            id: r.get(0)?,
            name: r.get(1)?,
            dir_name: r.get(2)?,
            deleted_at: r.get::<_, Option<String>>(3)?.map(|s| parse_ts(&s)),
            lead_bot_id: r.get(4)?,
            created_at: parse_ts(&r.get::<_, String>(5)?),
        })
    }

    pub fn create_project(&self, name: &str, dir_name: &str) -> anyhow::Result<Project> {
        let p = Project {
            id: new_id(),
            name: name.to_string(),
            dir_name: dir_name.to_string(),
            deleted_at: None,
            lead_bot_id: None,
            created_at: now(),
        };
        self.lock().execute(
            "INSERT INTO project(id, name, dir_name, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![p.id, p.name, p.dir_name, ts(p.created_at)],
        )?;
        Ok(p)
    }

    /// Name the bot that must know about every decision raised here.
    ///
    /// Not a gate: the lead cannot answer for the owner. It exists because a
    /// lead whose teammates ask the owner directly has a picture of its own
    /// project that silently goes stale.
    pub fn set_project_lead(&self, project_id: &str, bot_id: Option<&str>) -> anyhow::Result<()> {
        self.lock().execute(
            "UPDATE project SET lead_bot_id = ?2 WHERE id = ?1",
            params![project_id, bot_id],
        )?;
        Ok(())
    }

    /// The bot to tell about a decision in this project.
    ///
    /// Falls back to whoever created the raising bot — Chief of Staff hired
    /// Auction, Argus created Rigger — so a project that never set a lead
    /// still routes somewhere sensible rather than nowhere.
    pub fn lead_for_project(
        &self,
        project_id: &str,
        raised_by_bot_id: &str,
    ) -> anyhow::Result<Option<String>> {
        let named: Option<String> = self.lock().query_row(
            "SELECT lead_bot_id FROM project WHERE id = ?1",
            params![project_id],
            |r| r.get(0),
        )?;
        if named.is_some() {
            return Ok(named);
        }
        Ok(self
            .get_bot(raised_by_bot_id)?
            .and_then(|b| b.created_by_bot_id))
    }

    /// Live projects only; archived ones keep their rows but leave the roster.
    pub fn list_projects(&self) -> anyhow::Result<Vec<Project>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM project WHERE deleted_at IS NULL ORDER BY name",
            Self::PROJECT_COLS
        ))?;
        let rows = stmt.query_map([], Self::project_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Any project, archived or not — a bot archived with its project must
    /// still resolve to the directory it lives in.
    pub fn get_project(&self, id: &str) -> anyhow::Result<Option<Project>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                &format!("SELECT {} FROM project WHERE id = ?1", Self::PROJECT_COLS),
                params![id],
                Self::project_from_row,
            )
            .optional()?)
    }

    /// The project as a target for edits: archived rows do not qualify.
    pub fn get_live_project(&self, id: &str) -> anyhow::Result<Option<Project>> {
        Ok(self.get_project(id)?.filter(|p| p.deleted_at.is_none()))
    }

    /// True when a live project other than `except_id` already holds the name.
    pub fn project_name_taken(&self, name: &str, except_id: &str) -> anyhow::Result<bool> {
        let conn = self.lock();
        let found: Option<String> = conn
            .query_row(
                "SELECT id FROM project WHERE name = ?1 AND deleted_at IS NULL",
                params![name],
                |r| r.get(0),
            )
            .optional()?;
        Ok(found.is_some_and(|id| id != except_id))
    }

    /// True when any project — archived included — occupies the directory.
    /// Archived directories are kept, so their names stay reserved.
    pub fn project_dir_name_taken(&self, dir_name: &str) -> anyhow::Result<bool> {
        let conn = self.lock();
        let found: Option<String> = conn
            .query_row(
                "SELECT id FROM project WHERE dir_name = ?1",
                params![dir_name],
                |r| r.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// Rename a project. `dir_name` deliberately stays put, so nothing on disk
    /// moves and no bot's `workspace_path` goes stale.
    pub fn rename_project(&self, id: &str, name: &str) -> anyhow::Result<()> {
        self.lock().execute(
            "UPDATE project SET name = ?2 WHERE id = ?1",
            params![id, name],
        )?;
        Ok(())
    }

    /// Archive a project: it leaves `list_projects` and frees its name, but the
    /// row survives so its bots and their messages stay attributable.
    ///
    /// Callers are responsible for archiving the bots inside it first; see
    /// `projectmgmt::archive_project`.
    pub fn archive_project(&self, id: &str, deleted_by: &str) -> anyhow::Result<()> {
        let conn = self.lock();
        let name: String =
            conn.query_row("SELECT name FROM project WHERE id = ?1", params![id], |r| {
                r.get(0)
            })?;
        let suffix: String = id.chars().take(8).collect();
        let tombstone = format!("{name}{TOMBSTONE_SEP}{suffix}");
        conn.execute(
            "UPDATE project SET deleted_at = ?2, deleted_by = ?3, name = ?4 WHERE id = ?1",
            params![id, ts(now()), deleted_by, tombstone],
        )?;
        Ok(())
    }

    /// The original name of an archived project, or its current name if live.
    pub fn display_project_name(project: &Project) -> String {
        match project.deleted_at {
            Some(_) => project.name.rsplit_once(TOMBSTONE_SEP).map_or_else(
                || project.name.clone(),
                |(original, _)| original.to_string(),
            ),
            None => project.name.clone(),
        }
    }
}
