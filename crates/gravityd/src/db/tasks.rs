//! Task rows and hop accounting.

use anyhow::bail;
use bus::*;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

use super::{parse_ts, ts, Db};

impl Db {
    // ---- tasks ----

    pub fn create_task(
        &self,
        origin_message_id: &str,
        from_bot_id: Option<&str>,
        to_bot_id: &str,
        deadline_at: Option<DateTime<Utc>>,
        hop_count: i64,
        origin_chain: &str,
    ) -> anyhow::Result<Task> {
        if hop_count > MAX_TASK_HOPS {
            bail!(
                "task hop limit exceeded ({hop_count} > {MAX_TASK_HOPS}) — do the \
                 work yourself, or report what you have with complete_task"
            );
        }
        let t = Task {
            id: new_id(),
            origin_message_id: origin_message_id.to_string(),
            from_bot_id: from_bot_id.map(|s| s.to_string()),
            to_bot_id: to_bot_id.to_string(),
            state: TaskState::Open,
            deadline_at,
            hop_count,
            origin_chain: origin_chain.to_string(),
            reply_count: 0,
            created_at: now(),
        };
        self.lock().execute(
            "INSERT INTO task(id, origin_message_id, from_bot_id, to_bot_id, state, deadline_at, hop_count, origin_chain, created_at)
             VALUES (?1, ?2, ?3, ?4, 'open', ?5, ?6, ?7, ?8)",
            params![
                t.id,
                t.origin_message_id,
                t.from_bot_id,
                t.to_bot_id,
                t.deadline_at.map(ts),
                t.hop_count,
                t.origin_chain,
                ts(t.created_at)
            ],
        )?;
        Ok(t)
    }

    pub fn get_task(&self, task_id: &str) -> anyhow::Result<Option<Task>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                "SELECT id, origin_message_id, from_bot_id, to_bot_id, state, deadline_at, hop_count, origin_chain, reply_count, created_at
                 FROM task WHERE id = ?1",
                params![task_id],
                |r| {
                    let state: String = r.get(4)?;
                    Ok(Task {
                        id: r.get(0)?,
                        origin_message_id: r.get(1)?,
                        from_bot_id: r.get(2)?,
                        to_bot_id: r.get(3)?,
                        state: match state.as_str() {
                            "done" => TaskState::Done,
                            "cancelled" => TaskState::Cancelled,
                            "expired" => TaskState::Expired,
                            _ => TaskState::Open,
                        },
                        deadline_at: r.get::<_, Option<String>>(5)?.map(|s| parse_ts(&s)),
                        hop_count: r.get(6)?,
                        origin_chain: r.get(7)?,
                        reply_count: r.get(8)?,
                        created_at: parse_ts(&r.get::<_, String>(9)?),
                    })
                },
            )
            .optional()?)
    }

    /// The open task a message opened for its recipient, if any. Lets the
    /// daemon tell an assignee which task an inbound message is.
    pub fn open_task_for_message(
        &self,
        message_id: &str,
        to_bot_id: &str,
    ) -> anyhow::Result<Option<Task>> {
        let conn = self.lock();
        let id: Option<String> = conn
            .query_row(
                "SELECT id FROM task
                 WHERE origin_message_id = ?1 AND to_bot_id = ?2 AND state = 'open'
                 ORDER BY created_at DESC LIMIT 1",
                params![message_id, to_bot_id],
                |r| r.get(0),
            )
            .optional()?;
        drop(conn);
        match id {
            Some(id) => self.get_task(&id),
            None => Ok(None),
        }
    }

    pub fn newest_open_task_for(&self, bot_id: &str) -> anyhow::Result<Option<Task>> {
        let conn = self.lock();
        let id: Option<String> = conn
            .query_row(
                "SELECT id FROM task WHERE to_bot_id = ?1 AND state = 'open'
                 ORDER BY created_at DESC LIMIT 1",
                params![bot_id],
                |r| r.get(0),
            )
            .optional()?;
        drop(conn);
        match id {
            Some(id) => self.get_task(&id),
            None => Ok(None),
        }
    }

    /// Open tasks assigned to a bot, newest first. Used when archiving a bot:
    /// each one has a requester still waiting on a `complete_task` that can
    /// never arrive.
    pub fn open_tasks_for(&self, bot_id: &str) -> anyhow::Result<Vec<Task>> {
        let conn = self.lock();
        let ids: Vec<String> = conn
            .prepare(
                "SELECT id FROM task WHERE to_bot_id = ?1 AND state = 'open'
                 ORDER BY created_at DESC",
            )?
            .query_map(params![bot_id], |r| r.get(0))?
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

    /// The newest open task `from_bot_id` delegated to `to_bot_id`. Lets a
    /// requester's `reply` land on the task it opened instead of spawning a
    /// new delegation.
    pub fn newest_open_task_from(
        &self,
        from_bot_id: &str,
        to_bot_id: &str,
    ) -> anyhow::Result<Option<Task>> {
        let conn = self.lock();
        let id: Option<String> = conn
            .query_row(
                "SELECT id FROM task
                 WHERE from_bot_id = ?1 AND to_bot_id = ?2 AND state = 'open'
                 ORDER BY created_at DESC LIMIT 1",
                params![from_bot_id, to_bot_id],
                |r| r.get(0),
            )
            .optional()?;
        drop(conn);
        match id {
            Some(id) => self.get_task(&id),
            None => Ok(None),
        }
    }

    /// Atomically spend one unit of a task's reply budget. Returns false when
    /// the task is not open or the budget is exhausted — the caller refuses
    /// the send in that case.
    pub fn try_count_task_reply(&self, task_id: &str, max: i64) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE task SET reply_count = reply_count + 1
             WHERE id = ?1 AND state = 'open' AND reply_count < ?2",
            params![task_id, max],
        )?;
        Ok(changed == 1)
    }

    /// Open tasks a bot has delegated, newest first. `origin_chain` narrows to
    /// one chain position (the child chain the bot's next delegation would
    /// carry), because the fan-out budget is per chain rather than per bot.
    pub fn open_tasks_delegated_by(
        &self,
        from_bot_id: &str,
        origin_chain: Option<&str>,
    ) -> anyhow::Result<Vec<Task>> {
        let conn = self.lock();
        let ids: Vec<String> = match origin_chain {
            Some(chain) => conn
                .prepare(
                    "SELECT id FROM task
                     WHERE from_bot_id = ?1 AND state = 'open' AND origin_chain = ?2
                     ORDER BY created_at DESC",
                )?
                .query_map(params![from_bot_id, chain], |r| r.get(0))?
                .collect::<Result<_, _>>()?,
            None => conn
                .prepare(
                    "SELECT id FROM task
                     WHERE from_bot_id = ?1 AND state = 'open'
                     ORDER BY created_at DESC",
                )?
                .query_map(params![from_bot_id], |r| r.get(0))?
                .collect::<Result<_, _>>()?,
        };
        drop(conn);
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(task) = self.get_task(&id)? {
                out.push(task);
            }
        }
        Ok(out)
    }

    /// Open tasks whose deadline has passed. `deadline_at IS NULL` means no
    /// deadline; those never expire.
    pub fn overdue_open_tasks(&self, now: DateTime<Utc>) -> anyhow::Result<Vec<Task>> {
        let conn = self.lock();
        let ids: Vec<String> = conn
            .prepare(
                "SELECT id FROM task
                 WHERE state = 'open' AND deadline_at IS NOT NULL AND deadline_at < ?1",
            )?
            .query_map(params![ts(now)], |r| r.get(0))?
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

    /// Atomically move an open task to a terminal state. Returns false when it
    /// was not open — the flip is the gate that keeps the notice each closing
    /// path sends exactly-once, and stops two closers racing to overwrite each
    /// other's outcome.
    pub fn try_close_task(&self, task_id: &str, state: TaskState) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE task SET state = ?2 WHERE id = ?1 AND state = 'open'",
            params![task_id, state.as_str()],
        )?;
        Ok(changed == 1)
    }

    /// Flip an open task to expired. Returns false when the task was not open.
    pub fn expire_task(&self, task_id: &str) -> anyhow::Result<bool> {
        self.try_close_task(task_id, TaskState::Expired)
    }

    pub fn set_task_state(&self, task_id: &str, state: TaskState) -> anyhow::Result<()> {
        self.lock().execute(
            "UPDATE task SET state = ?2 WHERE id = ?1",
            params![task_id, state.as_str()],
        )?;
        Ok(())
    }
}
