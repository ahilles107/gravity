//! Peer daemons, linked bots, and the id mappings for messages and tasks that
//! cross a peer link. See `docs/peer-bots.md`.

use bus::*;
use rusqlite::{params, OptionalExtension, Row};

use super::{parse_ts, ts, Db};

/// `meta` key holding this daemon's stable id, which peers bind a token to.
const DAEMON_ID_KEY: &str = "daemon_id";

impl Db {
    // ---- this daemon ----

    /// This daemon's stable id, created the first time it is asked for.
    pub fn daemon_id(&self) -> anyhow::Result<String> {
        if let Some(id) = self.get_meta(DAEMON_ID_KEY)? {
            return Ok(id);
        }
        let id = new_id();
        self.set_meta(DAEMON_ID_KEY, &id)?;
        Ok(id)
    }

    // ---- peers ----

    fn peer_from_row(r: &Row<'_>) -> rusqlite::Result<Peer> {
        Ok(Peer {
            id: r.get(0)?,
            name: r.get(1)?,
            daemon_id: r.get(2)?,
            url: r.get(3)?,
            created_at: parse_ts(&r.get::<_, String>(4)?),
            last_seen_at: r.get::<_, Option<String>>(5)?.map(|s| parse_ts(&s)),
            revoked_at: r.get::<_, Option<String>>(6)?.map(|s| parse_ts(&s)),
        })
    }

    const PEER_COLS: &'static str =
        "id, name, daemon_id, url, created_at, last_seen_at, revoked_at";

    pub fn create_peer(&self, name: &str, url: Option<&str>) -> anyhow::Result<Peer> {
        let peer = Peer {
            id: new_id(),
            name: name.to_string(),
            daemon_id: None,
            url: url.map(str::to_string),
            created_at: now(),
            last_seen_at: None,
            revoked_at: None,
        };
        self.lock().execute(
            "INSERT INTO peer(id, name, url, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![peer.id, peer.name, peer.url, ts(peer.created_at)],
        )?;
        Ok(peer)
    }

    pub fn list_peers(&self) -> anyhow::Result<Vec<Peer>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM peer ORDER BY name",
            Self::PEER_COLS
        ))?;
        let rows = stmt.query_map([], Self::peer_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get_peer(&self, peer_id: &str) -> anyhow::Result<Option<Peer>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                &format!("SELECT {} FROM peer WHERE id = ?1", Self::PEER_COLS),
                params![peer_id],
                Self::peer_from_row,
            )
            .optional()?)
    }

    pub fn get_peer_by_name(&self, name: &str) -> anyhow::Result<Option<Peer>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                &format!(
                    "SELECT {} FROM peer WHERE name = ?1 COLLATE NOCASE",
                    Self::PEER_COLS
                ),
                params![name],
                Self::peer_from_row,
            )
            .optional()?)
    }

    /// Binds the peer to the daemon that presented its token. The first
    /// daemon wins; any other daemon presenting the same token is refused.
    pub fn bind_peer_daemon(&self, peer_id: &str, daemon_id: &str) -> anyhow::Result<bool> {
        let conn = self.lock();
        conn.execute(
            "UPDATE peer SET daemon_id = ?2 WHERE id = ?1 AND daemon_id IS NULL",
            params![peer_id, daemon_id],
        )?;
        let bound: Option<String> = conn.query_row(
            "SELECT daemon_id FROM peer WHERE id = ?1",
            params![peer_id],
            |r| r.get(0),
        )?;
        Ok(bound.as_deref() == Some(daemon_id))
    }

    pub fn touch_peer(&self, peer_id: &str) -> anyhow::Result<()> {
        self.lock().execute(
            "UPDATE peer SET last_seen_at = ?2 WHERE id = ?1",
            params![peer_id, ts(now())],
        )?;
        Ok(())
    }

    pub fn revoke_peer(&self, peer_id: &str) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE peer SET revoked_at = ?2 WHERE id = ?1 AND revoked_at IS NULL",
            params![peer_id, ts(now())],
        )?;
        Ok(changed > 0)
    }

    // ---- links ----

    /// Exposes a local bot to a peer: the peer may now deliver to it.
    pub fn expose_bot_to_peer(&self, peer_id: &str, bot_id: &str) -> anyhow::Result<()> {
        self.lock().execute(
            "INSERT OR IGNORE INTO peer_link(peer_id, bot_id) VALUES (?1, ?2)",
            params![peer_id, bot_id],
        )?;
        Ok(())
    }

    pub fn is_exposed_to_peer(&self, peer_id: &str, bot_id: &str) -> anyhow::Result<bool> {
        let conn = self.lock();
        let found: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM peer_link WHERE peer_id = ?1 AND bot_id = ?2",
                params![peer_id, bot_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }

    /// The live linked bot standing in for `remote_bot_id` in a project.
    pub fn linked_bot(
        &self,
        peer_id: &str,
        remote_bot_id: &str,
        project_id: &str,
    ) -> anyhow::Result<Option<Bot>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {} FROM bot WHERE peer_id = ?1 AND remote_bot_id = ?2 AND project_id = ?3 \
             AND deleted_at IS NULL",
            Self::BOT_COLS
        );
        Ok(conn
            .query_row(
                &sql,
                params![peer_id, remote_bot_id, project_id],
                Self::bot_from_row,
            )
            .optional()?)
    }

    /// A bot row standing in for a bot that runs on `peer_id`. It gets a DM
    /// conversation like any bot, an empty workspace path, and a directory
    /// name no local bot can have, so nothing ever provisions files for it.
    pub fn create_linked_bot(
        &self,
        project_id: &str,
        remote: &RemoteBot,
        peer_id: &str,
    ) -> anyhow::Result<Bot> {
        let dir_name = format!(".peer-{}", &new_id()[..8]);
        let mut bot = self.create_bot_with_runtime(
            project_id,
            &remote.name,
            &remote.description,
            "",
            &remote.avatar,
            "",
            &dir_name,
            None,
            remote.runtime,
        )?;
        self.lock().execute(
            "UPDATE bot SET peer_id = ?2, remote_bot_id = ?3 WHERE id = ?1",
            params![bot.id, peer_id, remote.id],
        )?;
        bot.peer_id = Some(peer_id.to_string());
        bot.remote_bot_id = Some(remote.id.clone());
        Ok(bot)
    }

    // ---- id mappings ----

    pub fn map_peer_message(
        &self,
        peer_id: &str,
        remote_message_id: &str,
        message_id: &str,
    ) -> anyhow::Result<()> {
        self.lock().execute(
            "INSERT OR IGNORE INTO peer_message(peer_id, remote_message_id, message_id)
             VALUES (?1, ?2, ?3)",
            params![peer_id, remote_message_id, message_id],
        )?;
        Ok(())
    }

    /// The local copy of a message the peer knows as `remote_message_id`.
    pub fn local_message_for(
        &self,
        peer_id: &str,
        remote_message_id: &str,
    ) -> anyhow::Result<Option<String>> {
        self.peer_lookup(
            "SELECT message_id FROM peer_message WHERE peer_id = ?1 AND remote_message_id = ?2",
            peer_id,
            remote_message_id,
        )
    }

    /// The peer's id for a local message, when the message crossed the link.
    pub fn remote_message_for(
        &self,
        peer_id: &str,
        message_id: &str,
    ) -> anyhow::Result<Option<String>> {
        self.peer_lookup(
            "SELECT remote_message_id FROM peer_message WHERE peer_id = ?1 AND message_id = ?2",
            peer_id,
            message_id,
        )
    }

    pub fn map_peer_task(
        &self,
        peer_id: &str,
        remote_task_id: &str,
        task_id: &str,
    ) -> anyhow::Result<()> {
        self.lock().execute(
            "INSERT OR IGNORE INTO peer_task(peer_id, remote_task_id, task_id)
             VALUES (?1, ?2, ?3)",
            params![peer_id, remote_task_id, task_id],
        )?;
        Ok(())
    }

    /// The peer's id for the other half of a mirrored task.
    pub fn remote_task_for(&self, peer_id: &str, task_id: &str) -> anyhow::Result<Option<String>> {
        self.peer_lookup(
            "SELECT remote_task_id FROM peer_task WHERE peer_id = ?1 AND task_id = ?2",
            peer_id,
            task_id,
        )
    }

    fn peer_lookup(&self, sql: &str, peer_id: &str, key: &str) -> anyhow::Result<Option<String>> {
        let conn = self.lock();
        Ok(conn
            .query_row(sql, params![peer_id, key], |r| r.get(0))
            .optional()?)
    }
}

impl Db {
    /// Every task a message opened, whatever its state. A result or a cancel
    /// crossing the link has to find the task it closes after the close.
    pub fn tasks_with_origin(&self, origin_message_id: &str) -> anyhow::Result<Vec<Task>> {
        let ids: Vec<String> = {
            let conn = self.lock();
            let mut stmt = conn.prepare("SELECT id FROM task WHERE origin_message_id = ?1")?;
            let rows = stmt.query_map(params![origin_message_id], |r| r.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        let mut tasks = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(task) = self.get_task(&id)? {
                tasks.push(task);
            }
        }
        Ok(tasks)
    }
}
