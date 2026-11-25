//! The owner-chosen order of the bots in a project.
//!
//! Order is fleet state rather than a view preference: every client attached
//! to the daemon shows the same roster, so it lives beside the bots it orders.

use std::collections::HashSet;

use bus::*;
use rusqlite::params;

use super::Db;
use crate::decisions::{not_found, DecisionError};

impl Db {
    /// Rewrite a project's roster so `bot_ids` come first, in that order.
    ///
    /// Live bots the caller did not mention keep their relative order after
    /// the listed ones. That makes a request from a client whose list is a
    /// moment stale — a bot just created by another bot, say — still land
    /// instead of failing over a bot the owner never saw.
    ///
    /// Positions are renumbered densely from zero in one transaction, so a
    /// concurrent reader sees the old order or the new one, never a mix.
    /// Returns the project's live bots in their new order.
    pub fn reorder_bots(&self, project_id: &str, bot_ids: &[String]) -> anyhow::Result<Vec<Bot>> {
        if self.get_live_project(project_id)?.is_none() {
            return Err(not_found("project not found"));
        }
        let mut seen = HashSet::new();
        for id in bot_ids {
            if !seen.insert(id.as_str()) {
                return Err(invalid(format!("bot '{id}' is listed more than once")));
            }
        }
        let current = self.list_bots(Some(project_id))?;
        for id in bot_ids {
            if current.iter().any(|bot| &bot.id == id) {
                continue;
            }
            return Err(match self.get_live_bot(id)? {
                Some(_) => invalid(format!("bot '{id}' belongs to another project")),
                None => not_found(format!("bot '{id}' not found")),
            });
        }

        let ordered: Vec<&str> = bot_ids
            .iter()
            .map(String::as_str)
            .chain(
                current
                    .iter()
                    .map(|bot| bot.id.as_str())
                    .filter(|id| !seen.contains(id)),
            )
            .collect();
        {
            let mut conn = self.lock();
            let tx = conn.transaction()?;
            for (position, id) in ordered.iter().enumerate() {
                tx.execute(
                    "UPDATE bot SET sort_order = ?2 WHERE id = ?1 AND project_id = ?3",
                    params![id, position as i64, project_id],
                )?;
            }
            tx.commit()?;
        }
        self.list_bots(Some(project_id))
    }
}

fn invalid(message: String) -> anyhow::Error {
    DecisionError::Invalid(message).into()
}
