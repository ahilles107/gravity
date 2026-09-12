//! Occurrences of a routine: scheduling, leasing, retry and completion.
//!
//! Identity is `(routine_id, scheduled_for)` for time triggers and
//! `(routine_id, signal_id)` for signal ones, both unique-constrained, which
//! is what makes creation idempotent across ticks, restarts and redelivery.

use bus::*;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension, Row};

use super::{parse_ts, ts, Db};

/// A new occurrence to insert.
pub struct NewRun<'a> {
    pub routine_id: &'a str,
    pub scheduled_for: DateTime<Utc>,
    pub source: RunSource,
    pub signal_id: Option<&'a str>,
}

pub struct RunDispatch {
    pub message: Message,
    pub delivery: Delivery,
}

fn qualified(cols: &str, alias: &str) -> String {
    cols.split(',')
        .map(|c| format!("{alias}.{}", c.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

impl Db {
    /// `None` means the occurrence already existed.
    pub fn schedule_routine_run(&self, run: NewRun<'_>) -> anyhow::Result<Option<RoutineRun>> {
        let conn = self.lock();
        let id = new_id();
        // No conflict target: either unique constraint may be the one hit.
        let n = conn.execute(
            "INSERT INTO routine_run(id, routine_id, scheduled_for, state, source, signal_id)
             VALUES (?1, ?2, ?3, 'scheduled', ?4, ?5)
             ON CONFLICT DO NOTHING",
            params![
                id,
                run.routine_id,
                ts(run.scheduled_for),
                run.source.as_str(),
                run.signal_id
            ],
        )?;
        if n == 0 {
            return Ok(None);
        }
        Ok(Some(RoutineRun {
            id,
            routine_id: run.routine_id.to_string(),
            scheduled_for: run.scheduled_for,
            state: RoutineRunState::Scheduled,
            source: run.source,
            attempt: 0,
            signal_id: run.signal_id.map(str::to_string),
            message_id: None,
            delivery_id: None,
            started_at: None,
            deadline_at: None,
            next_attempt_at: None,
            finished_at: None,
            error: None,
        }))
    }

    pub(super) fn run_from_row(r: &Row<'_>) -> rusqlite::Result<RoutineRun> {
        let state: String = r.get(3)?;
        let source: String = r.get(4)?;
        Ok(RoutineRun {
            id: r.get(0)?,
            routine_id: r.get(1)?,
            scheduled_for: parse_ts(&r.get::<_, String>(2)?),
            state: match state.as_str() {
                "running" => RoutineRunState::Running,
                "succeeded" => RoutineRunState::Succeeded,
                "failed" => RoutineRunState::Failed,
                "skipped" => RoutineRunState::Skipped,
                "cancelled" => RoutineRunState::Cancelled,
                _ => RoutineRunState::Scheduled,
            },
            source: RunSource::parse(&source).unwrap_or(RunSource::Schedule),
            attempt: r.get(5)?,
            signal_id: r.get(6)?,
            message_id: r.get(7)?,
            delivery_id: r.get(8)?,
            started_at: r.get::<_, Option<String>>(9)?.map(|s| parse_ts(&s)),
            deadline_at: r.get::<_, Option<String>>(10)?.map(|s| parse_ts(&s)),
            next_attempt_at: r.get::<_, Option<String>>(11)?.map(|s| parse_ts(&s)),
            finished_at: r.get::<_, Option<String>>(12)?.map(|s| parse_ts(&s)),
            error: r.get(13)?,
        })
    }

    pub(super) const RUN_COLS: &'static str =
        "id, routine_id, scheduled_for, state, source, attempt, \
         signal_id, message_id, delivery_id, started_at, deadline_at, next_attempt_at, \
         finished_at, error";

    /// Claim due occurrences under a lease that doubles as the run deadline.
    pub fn lease_due_routine_runs(&self, default_lease: i64) -> anyhow::Result<Vec<RoutineRun>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let due: Vec<(RoutineRun, i64)> = {
            let mut stmt = tx.prepare(&format!(
                "SELECT {}, COALESCE(rt.max_duration_seconds, ?2)
                   FROM routine_run r JOIN routine rt ON rt.id = r.routine_id
                  WHERE r.state = 'scheduled'
                    AND COALESCE(r.next_attempt_at, r.scheduled_for) <= ?1
                  ORDER BY r.scheduled_for LIMIT 50",
                qualified(Self::RUN_COLS, "r")
            ))?;
            let rows = stmt.query_map(params![ts(now()), default_lease], |row| {
                Ok((Self::run_from_row(row)?, row.get::<_, i64>(14)?))
            })?;
            rows.collect::<Result<_, _>>()?
        };
        let mut claimed = Vec::with_capacity(due.len());
        for (run, duration) in due {
            let started = now();
            let deadline = started + Duration::seconds(duration.max(1));
            tx.execute(
                "UPDATE routine_run
                    SET state = 'running', attempt = attempt + 1, started_at = ?2,
                        deadline_at = ?3, lease_until = ?3, next_attempt_at = NULL
                  WHERE id = ?1",
                params![run.id, ts(started), ts(deadline)],
            )?;
            claimed.push(RoutineRun {
                state: RoutineRunState::Running,
                attempt: run.attempt + 1,
                started_at: Some(started),
                deadline_at: Some(deadline),
                next_attempt_at: None,
                ..run
            });
        }
        tx.commit()?;
        Ok(claimed)
    }

    /// Returned rather than failed in place, so the caller can apply the
    /// routine's retry policy.
    pub fn overdue_routine_runs(&self) -> anyhow::Result<Vec<RoutineRun>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM routine_run WHERE state = 'running' AND lease_until < ?1",
            Self::RUN_COLS
        ))?;
        let rows = stmt.query_map(params![ts(now())], Self::run_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Atomically publish one attempt and attach it to a still-running run.
    /// Cancellation holds the same connection lock, so a cancelled occurrence
    /// can never leave an unattached delivery behind.
    pub fn dispatch_routine_run(
        &self,
        routine: &Routine,
        run: &RoutineRun,
        body: &str,
    ) -> anyhow::Result<Option<RunDispatch>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let running: bool = tx
            .query_row(
                "SELECT state = 'running' FROM routine_run WHERE id = ?1",
                params![run.id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(false);
        if !running {
            return Ok(None);
        }
        let conversation_id: String = tx.query_row(
            "SELECT id FROM conversation WHERE bot_id = ?1",
            params![routine.bot_id],
            |row| row.get(0),
        )?;
        let num: i64 =
            tx.query_row("SELECT COALESCE(MAX(num), 0) + 1 FROM message", [], |row| {
                row.get(0)
            })?;
        let created_at = now();
        let message = Message {
            id: new_id(),
            num,
            conversation_id,
            sender: Sender {
                kind: SenderKind::Routine,
                bot_id: None,
                name: routine.name.clone(),
            },
            kind: MessageKind::Task,
            body: body.to_string(),
            ref_message_id: None,
            decision_id: None,
            created_at,
        };
        tx.execute(
            "INSERT INTO message(id, num, conversation_id, sender_kind, sender_bot_id,
                                 sender_name, kind, body, ref_message_id, created_at)
             VALUES (?1, ?2, ?3, 'routine', NULL, ?4, 'task', ?5, NULL, ?6)",
            params![
                message.id,
                message.num,
                message.conversation_id,
                message.sender.name,
                message.body,
                ts(message.created_at)
            ],
        )?;
        let delivery = Delivery {
            id: new_id(),
            message_id: message.id.clone(),
            bot_id: routine.bot_id.clone(),
            state: DeliveryState::Queued,
            attempt_count: 0,
            next_attempt_at: created_at,
            last_error: None,
            created_at,
        };
        let key = format!("routine:{}:{}:attempt:{}", routine.id, run.id, run.attempt);
        tx.execute(
            "INSERT INTO delivery(id, message_id, bot_id, state, attempt_count,
                                  next_attempt_at, idempotency_key, created_at)
             VALUES (?1, ?2, ?3, 'queued', 0, ?4, ?5, ?6)",
            params![
                delivery.id,
                delivery.message_id,
                delivery.bot_id,
                ts(delivery.next_attempt_at),
                key,
                ts(delivery.created_at)
            ],
        )?;
        tx.execute(
            "UPDATE routine_run SET message_id = ?2, delivery_id = ?3
              WHERE id = ?1 AND state = 'running'",
            params![run.id, message.id, delivery.id],
        )?;
        tx.commit()?;
        Ok(Some(RunDispatch { message, delivery }))
    }
}
