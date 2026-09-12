//! Decision threads and the record of who was told about a ruling.

use bus::*;
use rusqlite::{params, Row};

use super::{parse_ts, ts, Db};

impl Db {
    fn comment_from_row(r: &Row<'_>) -> rusqlite::Result<DecisionComment> {
        let kind: String = r.get(2)?;
        Ok(DecisionComment {
            id: r.get(0)?,
            decision_id: r.get(1)?,
            author_kind: CommentAuthorKind::parse(&kind).unwrap_or(CommentAuthorKind::User),
            author_bot_id: r.get(3)?,
            // Resolved by the view layer: a bot may rename itself between
            // writing a comment and the owner reading it.
            author_name: String::new(),
            body: r.get(4)?,
            created_at: parse_ts(&r.get::<_, String>(5)?),
        })
    }

    pub fn insert_decision_comment(
        &self,
        decision_id: &str,
        author_kind: CommentAuthorKind,
        author_bot_id: Option<&str>,
        body: &str,
    ) -> anyhow::Result<DecisionComment> {
        let comment = DecisionComment {
            id: new_id(),
            decision_id: decision_id.to_string(),
            author_kind,
            author_bot_id: author_bot_id.map(|s| s.to_string()),
            author_name: String::new(),
            body: body.to_string(),
            created_at: now(),
        };
        self.lock().execute(
            "INSERT INTO decision_comment(id, decision_id, author_kind, author_bot_id, body,
                                          created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                comment.id,
                comment.decision_id,
                author_kind.as_str(),
                comment.author_bot_id,
                comment.body,
                ts(comment.created_at)
            ],
        )?;
        Ok(comment)
    }

    pub fn list_decision_comments(
        &self,
        decision_id: &str,
    ) -> anyhow::Result<Vec<DecisionComment>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, decision_id, author_kind, author_bot_id, body, created_at
               FROM decision_comment WHERE decision_id = ?1 ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![decision_id], Self::comment_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// How many comments each decision carries, and when the newest arrived.
    /// One query for a whole page, so a list never costs a query per row.
    pub fn comment_counts(
        &self,
        decision_ids: &[String],
    ) -> anyhow::Result<std::collections::HashMap<String, (i64, String)>> {
        if decision_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let placeholders = vec!["?"; decision_ids.len()].join(", ");
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT decision_id, count(*), max(created_at) FROM decision_comment
              WHERE decision_id IN ({placeholders}) GROUP BY decision_id"
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(decision_ids), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (id, count, last) = row?;
            out.insert(id, (count, last));
        }
        Ok(out)
    }

    /// Record that a bot was told about a settled decision.
    ///
    /// Returns false when it had already been told. Publish uses that to make
    /// one notification per bot per publish, so re-running a batch is safe.
    pub fn try_record_notification(
        &self,
        decision_id: &str,
        bot_id: &str,
        delivery_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "INSERT INTO decision_notification(decision_id, bot_id, delivery_id, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(decision_id, bot_id) DO NOTHING",
            params![decision_id, bot_id, delivery_id, ts(now())],
        )?;
        Ok(changed == 1)
    }

    /// Forget who was notified, so an edited ruling can go out again. The
    /// owner asked for a re-notify; silently dropping it would be worse than
    /// a bot hearing the same ruling twice.
    pub fn clear_notifications(&self, decision_id: &str) -> anyhow::Result<()> {
        self.lock().execute(
            "DELETE FROM decision_notification WHERE decision_id = ?1",
            params![decision_id],
        )?;
        Ok(())
    }

    pub fn list_notifications(
        &self,
        decision_id: &str,
    ) -> anyhow::Result<Vec<DecisionNotification>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT n.bot_id, b.name, n.delivery_id, n.created_at
               FROM decision_notification n JOIN bot b ON b.id = n.bot_id
              WHERE n.decision_id = ?1 ORDER BY n.created_at",
        )?;
        let rows = stmt.query_map(params![decision_id], |r| {
            Ok(DecisionNotification {
                bot_id: r.get(0)?,
                bot_name: r.get(1)?,
                delivery_id: r.get(2)?,
                created_at: parse_ts(&r.get::<_, String>(3)?),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
