//! A bot's tasks as its Tasks panel lists them: everything it was given or
//! delegated, open or closed, with the result that closed each one.

use bus::*;
use rusqlite::{params, OptionalExtension};

use super::Db;

impl Db {
    /// Tasks assigned to or delegated by a bot, newest first.
    pub fn tasks_involving(&self, bot_id: &str, limit: i64) -> anyhow::Result<Vec<Task>> {
        let conn = self.lock();
        let ids: Vec<String> = conn
            .prepare(
                "SELECT id FROM task WHERE to_bot_id = ?1 OR from_bot_id = ?1
                 ORDER BY created_at DESC LIMIT ?2",
            )?
            .query_map(params![bot_id, limit], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        drop(conn);
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(task) = self.get_task(&id)? {
                out.push(task);
            }
        }
        Ok(out)
    }

    /// The `done` message that reported a task's result, if one did.
    pub fn task_result(&self, origin_message_id: &str) -> anyhow::Result<Option<Message>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {} FROM message WHERE ref_message_id = ?1 AND kind = 'done'
             ORDER BY num DESC LIMIT 1",
            Self::MSG_COLS
        );
        Ok(conn
            .query_row(&sql, params![origin_message_id], Self::message_from_row)
            .optional()?)
    }
}
