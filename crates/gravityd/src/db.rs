use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::Context;
use bus::schema::MIGRATIONS;
use bus::*;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};

pub use revisions::Actor;
pub use routines::RoutineLimits;
pub use runs::NewRun;
pub use signals::NewSignal;

mod bots;
mod conversations;
mod deliveries;
mod devices;
mod projects;
mod revisions;
mod routines;
mod runs;
mod runs_state;
#[cfg(test)]
mod runs_tests;
mod settings;
mod signals;
mod tasks;
#[cfg(test)]
mod tests;

/// Handle to the SQLite bus. All access is serialized through one connection;
/// statements are short so contention stays negligible at this scale.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339()
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> anyhow::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> anyhow::Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", 5_000)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        db.backfill_dir_names()?;
        Ok(db)
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Give pre-migration-3 bots and pre-migration-7 projects the directory
    /// names the schema now requires.
    ///
    /// Both migrations can only default the column to `''`, and an empty
    /// `dir_name` would make `paths::bot_dir` resolve to the shared `bots/`
    /// directory — so every upgraded bot would appear to share one workspace.
    /// The stored `workspace_path` already encodes the real directory, so
    /// recover it from there and fall back to deriving it from the name.
    ///
    /// A project's directory has always been `names::dir_name(name)`, so
    /// deriving it is exact rather than a fallback.
    fn backfill_dir_names(&self) -> anyhow::Result<()> {
        self.backfill_project_dir_names()?;
        let conn = self.lock();
        let rows: Vec<(String, String, String)> = conn
            .prepare("SELECT id, name, workspace_path FROM bot WHERE dir_name = ''")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        for (id, name, workspace_path) in rows {
            let from_path = Path::new(&workspace_path)
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .filter(|n| !n.is_empty());
            let dir_name = from_path.unwrap_or_else(|| bus::names::dir_name(&name));
            conn.execute(
                "UPDATE bot SET dir_name = ?2 WHERE id = ?1",
                params![id, dir_name],
            )?;
        }
        Ok(())
    }

    fn backfill_project_dir_names(&self) -> anyhow::Result<()> {
        let conn = self.lock();
        let rows: Vec<(String, String)> = conn
            .prepare("SELECT id, name FROM project WHERE dir_name = ''")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        for (id, name) in rows {
            conn.execute(
                "UPDATE project SET dir_name = ?2 WHERE id = ?1",
                params![id, bus::names::dir_name(&name)],
            )?;
        }
        Ok(())
    }

    fn migrate(&self) -> anyhow::Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let has_meta: i64 = tx.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='meta'",
            [],
            |r| r.get(0),
        )?;
        let mut version: usize = if has_meta > 0 {
            tx.query_row(
                "SELECT value FROM meta WHERE key='schema_version'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
        } else {
            0
        };
        while version < MIGRATIONS.len() {
            tx.execute_batch(MIGRATIONS[version])
                .with_context(|| format!("applying migration {}", version + 1))?;
            version += 1;
        }
        tx.execute(
            "INSERT INTO meta(key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = ?1",
            params![version.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn integrity_check(&self) -> anyhow::Result<bool> {
        let conn = self.lock();
        let res: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        Ok(res == "ok")
    }

    // ---- retention ----

    /// Delete old rows per retention policy. Message deletion cascades into
    /// the FTS index via triggers. Returns (messages, deliveries, runs) pruned.
    ///
    /// Signals go last and only when no surviving run points at one, so a run
    /// kept for history never loses the record of what caused it.
    pub fn prune(
        &self,
        message_days: i64,
        delivery_days: i64,
        run_days: i64,
        signal_days: i64,
    ) -> anyhow::Result<(usize, usize, usize)> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let cutoff = |days: i64| ts(now() - Duration::days(days.max(1)));
        let runs = tx.execute(
            "DELETE FROM routine_run WHERE state IN ('succeeded', 'failed', 'skipped', 'cancelled')
             AND scheduled_for < ?1",
            params![cutoff(run_days)],
        )?;
        let deliveries = {
            tx.execute(
                "DELETE FROM inbox WHERE delivery_id IN
                   (SELECT id FROM delivery WHERE state IN ('acknowledged', 'delivered', 'failed')
                    AND created_at < ?1)",
                params![cutoff(delivery_days)],
            )?;
            tx.execute(
                "DELETE FROM delivery WHERE state IN ('acknowledged', 'delivered', 'failed')
                 AND created_at < ?1",
                params![cutoff(delivery_days)],
            )?
        };
        let messages = {
            // Only messages with no remaining delivery/task references.
            tx.execute(
                "DELETE FROM task WHERE state != 'open' AND created_at < ?1",
                params![cutoff(message_days)],
            )?;
            tx.execute(
                "UPDATE message SET ref_message_id = NULL WHERE ref_message_id IN
                   (SELECT id FROM message WHERE created_at < ?1
                    AND id NOT IN (SELECT message_id FROM delivery)
                    AND id NOT IN (SELECT origin_message_id FROM task))",
                params![cutoff(message_days)],
            )?;
            tx.execute(
                "DELETE FROM message WHERE created_at < ?1
                 AND id NOT IN (SELECT message_id FROM delivery)
                 AND id NOT IN (SELECT origin_message_id FROM task)
                 AND id NOT IN (SELECT ref_message_id FROM message WHERE ref_message_id IS NOT NULL)",
                params![cutoff(message_days)],
            )?
        };
        tx.commit()?;
        drop(conn);
        self.prune_signals(now() - Duration::days(signal_days.max(1)))?;
        Ok((messages, deliveries, runs))
    }

    /// Online backup to the given path using SQLite's backup API.
    pub fn backup_to(&self, dest: &Path) -> anyhow::Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = self.lock();
        let mut dst = Connection::open(dest)?;
        let backup = rusqlite::backup::Backup::new(&conn, &mut dst)?;
        backup.run_to_completion(64, std::time::Duration::from_millis(50), None)?;
        Ok(())
    }
}
