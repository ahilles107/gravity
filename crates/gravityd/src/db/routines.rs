//! Routines and their scheduled runs.

use bus::*;
use rusqlite::{params, OptionalExtension, Row};

use super::{parse_ts, ts, Db};

impl Db {
    // ---- routines ----

    fn routine_from_row(r: &Row<'_>) -> rusqlite::Result<Routine> {
        let trigger_json: String = r.get(3)?;
        let policy: String = r.get(5)?;
        Ok(Routine {
            id: r.get(0)?,
            bot_id: r.get(1)?,
            name: r.get(2)?,
            // Migration 8 retired `kind: "event"`; the sentinel never fires.
            trigger: serde_json::from_str(&trigger_json).unwrap_or_else(|_| Trigger::unparsable()),
            prompt: r.get(4)?,
            overlap_policy: OverlapPolicy::parse(&policy).unwrap_or(OverlapPolicy::Skip),
            enabled: r.get::<_, i64>(6)? != 0,
            max_duration_seconds: r.get(7)?,
            max_attempts: r.get(8)?,
            next_run_at: None,
            created_at: parse_ts(&r.get::<_, String>(9)?),
        })
    }

    const ROUTINE_COLS: &'static str = "id, bot_id, name, trigger_json, prompt, overlap_policy, \
         enabled, max_duration_seconds, max_attempts, created_at";

    const QUALIFIED_ROUTINE_COLS: &'static str =
        "r.id, r.bot_id, r.name, r.trigger_json, r.prompt, \
         r.overlap_policy, r.enabled, r.max_duration_seconds, r.max_attempts, r.created_at";

    fn signal_fields(trigger: &Trigger) -> (Option<&str>, Option<&str>) {
        match trigger {
            Trigger::Signal { name, from_bot_id } => (Some(name), from_bot_id.as_deref()),
            Trigger::Cron { .. } | Trigger::Interval { .. } => (None, None),
        }
    }

    pub fn create_routine(
        &self,
        bot_id: &str,
        name: &str,
        trigger: &Trigger,
        prompt: &str,
        overlap_policy: OverlapPolicy,
        enabled: bool,
    ) -> anyhow::Result<Routine> {
        let r = Routine {
            id: new_id(),
            bot_id: bot_id.to_string(),
            name: name.to_string(),
            trigger: trigger.clone(),
            prompt: prompt.to_string(),
            overlap_policy,
            enabled,
            max_duration_seconds: None,
            max_attempts: 1,
            next_run_at: None,
            created_at: now(),
        };
        let (signal_name, signal_from_bot_id) = Self::signal_fields(trigger);
        self.lock().execute(
            "INSERT INTO routine(id, bot_id, name, trigger_json, prompt, overlap_policy, enabled,
                                 max_attempts, created_at, signal_name, signal_from_bot_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                r.id,
                r.bot_id,
                r.name,
                serde_json::to_string(trigger)?,
                r.prompt,
                overlap_policy.as_str(),
                enabled as i64,
                r.max_attempts,
                ts(r.created_at),
                signal_name,
                signal_from_bot_id
            ],
        )?;
        Ok(r)
    }

    pub fn list_routines(&self, bot_id: Option<&str>) -> anyhow::Result<Vec<Routine>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {} FROM routine {} ORDER BY name",
            Self::ROUTINE_COLS,
            if bot_id.is_some() {
                "WHERE bot_id = ?1"
            } else {
                ""
            }
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = match bot_id {
            Some(b) => stmt.query_map(params![b], Self::routine_from_row)?,
            None => stmt.query_map([], Self::routine_from_row)?,
        };
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get_routine(&self, id: &str) -> anyhow::Result<Option<Routine>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                &format!("SELECT {} FROM routine WHERE id = ?1", Self::ROUTINE_COLS),
                params![id],
                Self::routine_from_row,
            )
            .optional()?)
    }

    /// Enabled signal subscribers in one project, filtered entirely in SQL.
    pub fn signal_routines(
        &self,
        project_id: &str,
        name: &str,
        from_bot_id: Option<&str>,
    ) -> anyhow::Result<Vec<Routine>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM routine r
               JOIN bot b ON b.id = r.bot_id
              WHERE b.project_id = ?1 AND b.deleted_at IS NULL
                AND r.enabled = 1 AND r.signal_name = ?2
                AND (r.signal_from_bot_id IS NULL OR r.signal_from_bot_id = ?3)
              ORDER BY r.name",
            Self::QUALIFIED_ROUTINE_COLS
        ))?;
        let rows = stmt.query_map(
            params![project_id, name, from_bot_id],
            Self::routine_from_row,
        )?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn set_routine_enabled(&self, id: &str, enabled: bool) -> anyhow::Result<()> {
        self.lock().execute(
            "UPDATE routine SET enabled = ?2 WHERE id = ?1",
            params![id, enabled as i64],
        )?;
        Ok(())
    }

    /// Partial update: `None` fields keep their current value.
    pub fn update_routine(
        &self,
        id: &str,
        name: Option<&str>,
        trigger: Option<&Trigger>,
        prompt: Option<&str>,
        overlap_policy: Option<OverlapPolicy>,
        limits: RoutineLimits,
    ) -> anyhow::Result<()> {
        let trigger_json = trigger.map(serde_json::to_string).transpose()?;
        let (signal_name, signal_from_bot_id) =
            trigger.map(Self::signal_fields).unwrap_or((None, None));
        self.lock().execute(
            "UPDATE routine SET
                 name = COALESCE(?2, name),
                 trigger_json = COALESCE(?3, trigger_json),
                 prompt = COALESCE(?4, prompt),
                 overlap_policy = COALESCE(?5, overlap_policy),
                 max_duration_seconds = COALESCE(?6, max_duration_seconds),
                 max_attempts = COALESCE(?7, max_attempts),
                 signal_name = CASE WHEN ?3 IS NULL THEN signal_name ELSE ?8 END,
                 signal_from_bot_id = CASE WHEN ?3 IS NULL THEN signal_from_bot_id ELSE ?9 END
             WHERE id = ?1",
            params![
                id,
                name,
                trigger_json,
                prompt,
                overlap_policy.map(|p| p.as_str()),
                limits.max_duration_seconds,
                limits.max_attempts,
                signal_name,
                signal_from_bot_id
            ],
        )?;
        Ok(())
    }

    /// Cancel queued occurrences, e.g. when the trigger they were computed
    /// from changes. Running occurrences are left to finish.
    pub fn cancel_scheduled_runs(&self, routine_id: &str, reason: &str) -> anyhow::Result<u64> {
        let n = self.lock().execute(
            "UPDATE routine_run SET state = 'cancelled', finished_at = ?2, error = ?3
             WHERE routine_id = ?1 AND state = 'scheduled'",
            params![routine_id, ts(now()), reason],
        )?;
        Ok(n as u64)
    }

    /// Remove a routine and its entire run history.
    pub fn delete_routine(&self, id: &str) -> anyhow::Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM routine_run WHERE routine_id = ?1", params![id])?;
        tx.execute("DELETE FROM routine WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }
}

/// Optional per-routine execution bounds, as a partial update.
#[derive(Debug, Clone, Copy, Default)]
pub struct RoutineLimits {
    pub max_duration_seconds: Option<i64>,
    pub max_attempts: Option<i64>,
}
