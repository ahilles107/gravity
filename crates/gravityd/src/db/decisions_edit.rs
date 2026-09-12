//! Editing and deleting decision records — owner-only operations.

use bus::*;
use rusqlite::params;

use super::{ts, Db};

/// The editable face of a decision. The caller reads the record, applies its
/// patch in Rust, and hands back final values, so clearing a deadline is just
/// `None` rather than a tri-state every query has to carry.
pub struct DecisionEdit {
    pub title: String,
    pub body: String,
    pub options: Vec<DecisionOption>,
    pub recommendation: Option<String>,
    pub priority: Priority,
    pub deadline_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ruling: Option<Ruling>,
}

impl Db {
    /// Overwrite the editable fields of a decision.
    ///
    /// Every edit is stamped, but previous values are not versioned: a real
    /// change of mind is a reopen that supersedes, which keeps both the old
    /// ruling and the reason it stopped applying.
    pub fn update_decision(
        &self,
        id: &str,
        edit: &DecisionEdit,
        edited_by: &str,
    ) -> anyhow::Result<bool> {
        let options = serde_json::to_string(&edit.options)?;
        let ruling = edit.ruling.as_ref();
        let changed = self.lock().execute(
            "UPDATE decision
                SET title = ?2, normalised_title = ?3, body = ?4, options_json = ?5,
                    recommendation = ?6, priority = ?7, deadline_at = ?8,
                    ruling_option = ?9, ruling_text = ?10, ruling_reason = ?11,
                    edited_at = ?12, edited_by = ?13
              WHERE id = ?1",
            params![
                id,
                edit.title,
                super::decisions::normalise_title(&edit.title),
                edit.body,
                options,
                edit.recommendation,
                edit.priority.as_str(),
                edit.deadline_at.map(ts),
                ruling.and_then(|r| r.option.clone()),
                ruling.map(|r| r.text.clone()),
                ruling.and_then(|r| r.reason.clone()),
                ts(now()),
                edited_by
            ],
        )?;
        Ok(changed == 1)
    }

    /// Delete a decision and everything that hangs off it.
    ///
    /// Messages already delivered keep their place in a bot's history: their
    /// `decision_id` is cleared rather than the message removed, because the
    /// bot read that ruling and acted on it whether or not the record survives.
    pub fn delete_decision(&self, id: &str) -> anyhow::Result<bool> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE message SET decision_id = NULL WHERE decision_id = ?1",
            params![id],
        )?;
        tx.execute(
            "UPDATE decision SET supersedes_id = NULL WHERE supersedes_id = ?1",
            params![id],
        )?;
        tx.execute(
            "UPDATE decision SET superseded_by_id = NULL WHERE superseded_by_id = ?1",
            params![id],
        )?;
        tx.execute(
            "DELETE FROM decision_notification WHERE decision_id = ?1",
            params![id],
        )?;
        tx.execute(
            "DELETE FROM decision_tag WHERE decision_id = ?1",
            params![id],
        )?;
        tx.execute(
            "DELETE FROM decision_comment WHERE decision_id = ?1",
            params![id],
        )?;
        let changed = tx.execute("DELETE FROM decision WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(changed == 1)
    }
}
