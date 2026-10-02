//! Worker rows: the spawn queue, and the temporary bots that run them.

use bus::*;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension, Row};

use super::{parse_ts, ts, Db};

/// A spawn as asked for, before it is queued.
pub struct NewWorker<'a> {
    pub project_id: &'a str,
    pub parent_bot_id: &'a str,
    pub name: &'a str,
    pub brief: &'a str,
    pub description: &'a str,
    pub instructions: &'a str,
    pub runtime: Option<BotRuntime>,
    pub machine: Option<&'a str>,
    pub deadline_hours: i64,
}

const WORKER_COLS: &str = "id, project_id, parent_bot_id, name, brief, description, \
     instructions, runtime, machine, deadline_hours, state, bot_id, task_id, error, \
     created_at, started_at, finished_at";

/// How long a temporary bot may sit without ever being given a task before it
/// is retired. Covers a worker created on this machine by a peer whose task
/// never arrived.
pub(super) const UNTASKED_GRACE_MINUTES: i64 = 10;

fn worker_from_row(r: &Row<'_>) -> rusqlite::Result<Worker> {
    let ts_opt = |s: Option<String>| s.map(|s| parse_ts(&s));
    Ok(Worker {
        id: r.get(0)?,
        project_id: r.get(1)?,
        parent_bot_id: r.get(2)?,
        name: r.get(3)?,
        brief: r.get(4)?,
        description: r.get(5)?,
        instructions: r.get(6)?,
        runtime: r
            .get::<_, Option<String>>(7)?
            .map(|raw| match raw.as_str() {
                "codex_cli" => BotRuntime::CodexCli,
                _ => BotRuntime::ClaudeCode,
            }),
        machine: r.get(8)?,
        deadline_hours: r.get(9)?,
        state: WorkerState::parse(&r.get::<_, String>(10)?).unwrap_or(WorkerState::Failed),
        bot_id: r.get(11)?,
        task_id: r.get(12)?,
        error: r.get(13)?,
        created_at: parse_ts(&r.get::<_, String>(14)?),
        started_at: ts_opt(r.get(15)?),
        finished_at: ts_opt(r.get(16)?),
    })
}

impl Db {
    // ---- workers ----

    pub fn set_bot_temporary(&self, bot_id: &str, temporary: bool) -> anyhow::Result<()> {
        self.lock().execute(
            "UPDATE bot SET temporary = ?2 WHERE id = ?1",
            params![bot_id, temporary],
        )?;
        Ok(())
    }

    /// Live workers that run here in a project — the number the worker cap
    /// applies to.
    pub fn count_live_workers(&self, project_id: &str) -> anyhow::Result<i64> {
        Ok(self.lock().query_row(
            "SELECT count(*) FROM bot
             WHERE project_id = ?1 AND peer_id IS NULL AND temporary = 1
               AND deleted_at IS NULL",
            params![project_id],
            |r| r.get(0),
        )?)
    }

    /// Whether a queued spawn has reserved this name in the project.
    pub fn worker_name_reserved(&self, project_id: &str, name: &str) -> anyhow::Result<bool> {
        let count: i64 = self.lock().query_row(
            "SELECT count(*) FROM worker
             WHERE project_id = ?1 AND state = 'queued' AND name = ?2 COLLATE NOCASE",
            params![project_id, name],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn insert_worker(&self, new: &NewWorker<'_>) -> anyhow::Result<Worker> {
        let id = new_id();
        self.lock().execute(
            "INSERT INTO worker(id, project_id, parent_bot_id, name, brief, description,
                                instructions, runtime, machine, deadline_hours, state, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'queued', ?11)",
            params![
                id,
                new.project_id,
                new.parent_bot_id,
                new.name,
                new.brief,
                new.description,
                new.instructions,
                new.runtime.map(BotRuntime::as_str),
                new.machine,
                new.deadline_hours,
                ts(now())
            ],
        )?;
        self.get_worker(&id)?
            .ok_or_else(|| anyhow::anyhow!("worker vanished after insert"))
    }

    pub fn get_worker(&self, id: &str) -> anyhow::Result<Option<Worker>> {
        let sql = format!("SELECT {WORKER_COLS} FROM worker WHERE id = ?1");
        Ok(self
            .lock()
            .query_row(&sql, params![id], worker_from_row)
            .optional()?)
    }

    fn workers_where(
        &self,
        filter: &str,
        args: &[&dyn rusqlite::ToSql],
    ) -> anyhow::Result<Vec<Worker>> {
        let sql = format!("SELECT {WORKER_COLS} FROM worker WHERE {filter}");
        let conn = self.lock();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(args, worker_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// A project's queue, oldest first: the order slots are handed out in.
    pub fn queued_workers(&self, project_id: &str) -> anyhow::Result<Vec<Worker>> {
        self.workers_where(
            "project_id = ?1 AND state = 'queued' ORDER BY created_at, rowid",
            &[&project_id],
        )
    }

    /// Projects with anything queued.
    pub fn projects_with_queued_workers(&self) -> anyhow::Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare("SELECT DISTINCT project_id FROM worker WHERE state = 'queued'")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn running_workers(&self) -> anyhow::Result<Vec<Worker>> {
        self.workers_where("state = 'running'", &[])
    }

    /// Every spawn still queued or running whose parent has been archived.
    pub fn orphaned_workers(&self) -> anyhow::Result<Vec<Worker>> {
        self.workers_where(
            "state IN ('queued', 'running')
               AND parent_bot_id IN (SELECT id FROM bot WHERE deleted_at IS NOT NULL)",
            &[],
        )
    }

    /// A parent's spawns: everything queued or running, and the most recent
    /// that finished.
    pub fn workers_of(&self, parent_bot_id: &str, finished: i64) -> anyhow::Result<Vec<Worker>> {
        let mut active = self.workers_where(
            "parent_bot_id = ?1 AND state IN ('queued', 'running') ORDER BY created_at, rowid",
            &[&parent_bot_id],
        )?;
        let mut done = self.workers_where(
            "parent_bot_id = ?1 AND state NOT IN ('queued', 'running')
             ORDER BY finished_at DESC, rowid DESC LIMIT ?2",
            &[&parent_bot_id, &finished],
        )?;
        done.reverse();
        active.extend(done);
        Ok(active)
    }

    /// A parent's queued or running spawn, by the name it was given.
    pub fn active_worker_by_name(
        &self,
        parent_bot_id: &str,
        name: &str,
    ) -> anyhow::Result<Option<Worker>> {
        Ok(self
            .workers_where(
                "parent_bot_id = ?1 AND name = ?2 COLLATE NOCASE
                   AND state IN ('queued', 'running')",
                &[&parent_bot_id, &name],
            )?
            .into_iter()
            .next())
    }

    /// The spawn a running bot belongs to.
    pub fn worker_for_bot(&self, bot_id: &str) -> anyhow::Result<Option<Worker>> {
        Ok(self
            .workers_where("bot_id = ?1 ORDER BY created_at DESC", &[&bot_id])?
            .into_iter()
            .next())
    }

    /// 1-based place in the project's queue, or `None` once it has left it.
    pub fn queue_position(&self, worker: &Worker) -> anyhow::Result<Option<i64>> {
        if worker.state != WorkerState::Queued {
            return Ok(None);
        }
        let ahead: i64 = self.lock().query_row(
            "SELECT count(*) FROM worker
             WHERE project_id = ?1 AND state = 'queued'
               AND rowid < (SELECT rowid FROM worker WHERE id = ?2)",
            params![worker.project_id, worker.id],
            |r| r.get(0),
        )?;
        Ok(Some(ahead + 1))
    }

    /// Move a queued spawn to running. False when it already left the queue,
    /// which is what keeps two dispatches from starting it twice.
    pub fn start_worker(&self, id: &str, bot_id: &str, task_id: &str) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE worker SET state = 'running', bot_id = ?2, task_id = ?3, started_at = ?4
             WHERE id = ?1 AND state = 'queued'",
            params![id, bot_id, task_id, ts(now())],
        )?;
        Ok(changed == 1)
    }

    /// Close a queued or running spawn. False when it was already final.
    pub fn finish_worker(
        &self,
        id: &str,
        state: WorkerState,
        error: Option<&str>,
    ) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE worker SET state = ?2, error = coalesce(?3, error), finished_at = ?4
             WHERE id = ?1 AND state IN ('queued', 'running')",
            params![id, state.as_str(), error, ts(now())],
        )?;
        Ok(changed == 1)
    }

    /// Record why a queued spawn is still waiting, without moving it.
    pub fn note_worker_waiting(&self, id: &str, reason: Option<&str>) -> anyhow::Result<()> {
        self.lock().execute(
            "UPDATE worker SET error = ?2 WHERE id = ?1 AND state = 'queued'",
            params![id, reason],
        )?;
        Ok(())
    }

    /// Temporary bots running here whose work is over: every task given to
    /// them is closed and everything they sent across a peer link has left,
    /// or they never got a task at all within the grace period.
    ///
    /// Waiting for that mail matters for a worker serving a peer: archiving
    /// it sends a roster that archives its stand-in there, and a stand-in
    /// archived before the result arrives cancels the task instead. Mail to a
    /// bot here is already stored, so it holds nothing up.
    pub fn finished_temporary_bots(&self, now: DateTime<Utc>) -> anyhow::Result<Vec<Bot>> {
        let cutoff = ts(now - Duration::minutes(UNTASKED_GRACE_MINUTES));
        let sql = format!(
            "SELECT {} FROM bot b
             WHERE b.temporary = 1 AND b.peer_id IS NULL AND b.deleted_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM task t WHERE t.to_bot_id = b.id AND t.state = 'open')
               AND NOT EXISTS (
                   SELECT 1 FROM delivery d
                   JOIN message m ON m.id = d.message_id
                   JOIN bot r ON r.id = d.bot_id
                   WHERE m.sender_bot_id = b.id AND r.peer_id IS NOT NULL
                     AND d.state IN ('queued', 'leased'))
               AND (EXISTS (SELECT 1 FROM task t WHERE t.to_bot_id = b.id)
                    OR b.created_at < ?1)",
            Self::BOT_COLS
                .split(", ")
                .map(|c| format!("b.{}", c.trim()))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let conn = self.lock();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![cutoff], Self::bot_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
