//! Bot identity revisions: the audit trail behind unsupervised self-management.
//!
//! Bots change their own name, avatar, description and instructions without
//! asking, and create and delete each other. Nothing prompts the user, so this
//! table is what keeps that legible and — for field edits — reversible.

use bus::*;
use rusqlite::{params, OptionalExtension, Row};

use super::{parse_ts, ts, Actor, Db};

impl Db {
    fn revision_from_row(r: &Row<'_>) -> rusqlite::Result<BotRevision> {
        let field: String = r.get(3)?;
        Ok(BotRevision {
            id: r.get(0)?,
            bot_id: r.get(1)?,
            changed_by: r.get(2)?,
            field: RevisionField::parse(&field).unwrap_or(RevisionField::Description),
            old_value: r.get(4)?,
            new_value: r.get(5)?,
            created_at: parse_ts(&r.get::<_, String>(6)?),
        })
    }

    const REVISION_COLS: &'static str =
        "id, bot_id, changed_by, field, old_value, new_value, created_at";

    pub fn record_revision(
        &self,
        bot_id: &str,
        changed_by: &Actor<'_>,
        field: RevisionField,
        old_value: &str,
        new_value: &str,
    ) -> anyhow::Result<BotRevision> {
        let rev = BotRevision {
            id: new_id(),
            bot_id: bot_id.to_string(),
            changed_by: changed_by.as_stored(),
            field,
            old_value: old_value.to_string(),
            new_value: new_value.to_string(),
            created_at: now(),
        };
        self.lock().execute(
            "INSERT INTO bot_revision(id, bot_id, changed_by, field, old_value, new_value, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                rev.id,
                rev.bot_id,
                rev.changed_by,
                rev.field.as_str(),
                rev.old_value,
                rev.new_value,
                ts(rev.created_at)
            ],
        )?;
        Ok(rev)
    }

    pub fn list_bot_revisions(&self, bot_id: &str, limit: i64) -> anyhow::Result<Vec<BotRevision>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {} FROM bot_revision WHERE bot_id = ?1
             ORDER BY created_at DESC, rowid DESC LIMIT ?2",
            Self::REVISION_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params![bot_id, limit.clamp(1, 500)],
            Self::revision_from_row,
        )?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get_bot_revision(&self, revision_id: &str) -> anyhow::Result<Option<BotRevision>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {} FROM bot_revision WHERE id = ?1",
            Self::REVISION_COLS
        );
        Ok(conn
            .query_row(&sql, params![revision_id], Self::revision_from_row)
            .optional()?)
    }

    /// Drop revisions older than the retention window, keeping lifecycle
    /// markers so a bot's creation and deletion stay visible for as long as the
    /// bot row does.
    pub fn prune_revisions(&self, days: i64) -> anyhow::Result<usize> {
        let cutoff = ts(now() - chrono::Duration::days(days.max(1)));
        Ok(self.lock().execute(
            "DELETE FROM bot_revision
             WHERE created_at < ?1 AND field NOT IN ('created', 'deleted')",
            params![cutoff],
        )?)
    }
}
