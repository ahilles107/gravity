//! Recording what became of an occurrence, and reading occurrences back.

use bus::*;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

use super::{ts, Db};

impl Db {
    pub fn finish_scheduled_routine_run(
        &self,
        run_id: &str,
        state: RoutineRunState,
        error: Option<&str>,
    ) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE routine_run SET state = ?2, finished_at = ?3, error = ?4, lease_until = NULL
              WHERE id = ?1 AND state = 'scheduled'",
            params![run_id, state.as_str(), ts(now()), error],
        )?;
        Ok(changed > 0)
    }

    pub fn finish_running_routine_run(
        &self,
        run_id: &str,
        state: RoutineRunState,
        error: Option<&str>,
    ) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE routine_run SET state = ?2, finished_at = ?3, error = ?4, lease_until = NULL
              WHERE id = ?1 AND state = 'running'",
            params![run_id, state.as_str(), ts(now()), error],
        )?;
        Ok(changed > 0)
    }

    /// Another attempt: `scheduled_for` and the attempt count stay put, only
    /// the wait moves.
    pub fn retry_routine_run(
        &self,
        run_id: &str,
        next_attempt_at: DateTime<Utc>,
        error: &str,
    ) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE routine_run
                SET state = 'scheduled', next_attempt_at = ?2, error = ?3,
                    lease_until = NULL, deadline_at = NULL, started_at = NULL
              WHERE id = ?1 AND state = 'running'",
            params![run_id, ts(next_attempt_at), error],
        )?;
        Ok(changed > 0)
    }

    /// Atomically cancel an unfinished run and any prompt that has not reached
    /// the bot yet. Returns false if another terminal transition won the race.
    pub fn cancel_routine_run(&self, run_id: &str, reason: &str) -> anyhow::Result<bool> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let delivery_id: Option<String> = tx
            .query_row(
                "SELECT delivery_id FROM routine_run
                  WHERE id = ?1 AND state IN ('scheduled', 'running')",
                params![run_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if let Some(delivery_id) = delivery_id {
            tx.execute(
                "UPDATE delivery SET state = 'failed', lease_until = NULL, last_error = ?2
                  WHERE id = ?1 AND state IN ('queued', 'leased')",
                params![delivery_id, reason],
            )?;
        }
        let changed = tx.execute(
            "UPDATE routine_run
                SET state = 'cancelled', finished_at = ?2, error = ?3, lease_until = NULL
              WHERE id = ?1 AND state IN ('scheduled', 'running')",
            params![run_id, ts(now()), reason],
        )?;
        tx.commit()?;
        Ok(changed > 0)
    }

    /// Requeue without consuming an attempt: blocked by the overlap policy is
    /// not a failure. Mirrors `defer_delivery`.
    pub fn defer_routine_run(&self, run_id: &str, reason: &str) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE routine_run
                SET state = 'scheduled', attempt = MAX(attempt - 1, 0), error = ?2,
                    next_attempt_at = NULL, lease_until = NULL, deadline_at = NULL,
                    started_at = NULL
              WHERE id = ?1 AND state = 'running'",
            params![run_id, reason],
        )?;
        Ok(changed > 0)
    }

    pub fn running_run_count(&self, routine_id: &str) -> anyhow::Result<i64> {
        let conn = self.lock();
        Ok(conn.query_row(
            "SELECT count(*) FROM routine_run WHERE routine_id = ?1 AND state = 'running'",
            params![routine_id],
            |r| r.get(0),
        )?)
    }

    pub fn pending_run_count(&self, routine_id: &str) -> anyhow::Result<i64> {
        let conn = self.lock();
        Ok(conn.query_row(
            "SELECT count(*) FROM routine_run WHERE routine_id = ?1 AND state = 'scheduled'",
            params![routine_id],
            |r| r.get(0),
        )?)
    }

    pub fn list_routine_runs(
        &self,
        routine_id: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<RoutineRun>> {
        let limit = limit.clamp(1, crate::routine_validation::MAX_RUN_HISTORY_LIMIT);
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM routine_run WHERE routine_id = ?1 ORDER BY scheduled_for DESC LIMIT ?2",
            Self::RUN_COLS
        ))?;
        let rows = stmt.query_map(params![routine_id, limit], Self::run_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get_routine_run(&self, id: &str) -> anyhow::Result<Option<RoutineRun>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                &format!("SELECT {} FROM routine_run WHERE id = ?1", Self::RUN_COLS),
                params![id],
                Self::run_from_row,
            )
            .optional()?)
    }

    /// Complete the exact run id found in the finished turn's transcript.
    pub fn complete_routine_run_for_bot(&self, run_id: &str, bot_id: &str) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE routine_run
                SET state = 'succeeded', finished_at = ?3, error = NULL, lease_until = NULL
              WHERE id = ?1 AND state = 'running'
                AND routine_id IN (SELECT id FROM routine WHERE bot_id = ?2)",
            params![run_id, bot_id, ts(now())],
        )?;
        Ok(changed > 0)
    }

    pub fn run_id_for_delivery(&self, delivery_id: &str) -> anyhow::Result<Option<String>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                "SELECT id FROM routine_run WHERE delivery_id = ?1",
                params![delivery_id],
                |row| row.get(0),
            )
            .optional()?)
    }
}
