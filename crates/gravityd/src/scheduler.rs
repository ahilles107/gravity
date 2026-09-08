//! Routine scheduler: one loop derives due occurrences from time triggers,
//! leases them under a deadline, applies the overlap policy, dispatches the
//! prompt, and retires the occurrence when the deadline passes or the bot
//! finishes the turn it started.

use std::str::FromStr;

use bus::{RoutineRunState, Trigger};
use chrono::{DateTime, Duration, Utc};

use crate::config::SchedulerConfig;
use crate::db::{Db, NewRun};
use crate::events::{Events, Internal, Push};

mod dispatch;
mod signals;

pub use dispatch::{cancel_run, dispatch_routine};
pub use signals::{emit_signal, validate_signal_name, EmitSignal, MAX_SIGNAL_HOPS};

pub struct Scheduler {
    pub db: Db,
    pub events: Events,
    pub cfg: SchedulerConfig,
}

/// Next occurrence of a time-based trigger after `from`, in UTC.
pub fn next_occurrence(
    trigger: &Trigger,
    from: DateTime<Utc>,
    anchor: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    match trigger {
        Trigger::Cron { expr, tz } => {
            let schedule = cron::Schedule::from_str(expr).ok()?;
            let tz: chrono_tz::Tz = tz.parse().unwrap_or(chrono_tz::UTC);
            schedule
                .after(&from.with_timezone(&tz))
                .next()
                .map(|t| t.with_timezone(&Utc))
        }
        Trigger::Interval { seconds } => {
            // First occurrence is one full interval after the anchor
            // (creation time); never at the anchor itself. All arithmetic is
            // checked: `validate_trigger` bounds new triggers, but a stored
            // value must never be able to panic the scheduler task.
            let secs = i64::try_from(*seconds).ok()?.max(1);
            let elapsed = (from - anchor).num_seconds().max(0);
            let k = elapsed / secs + 1;
            let step = Duration::try_seconds(k.checked_mul(secs)?)?;
            anchor.checked_add_signed(step)
        }
        Trigger::Signal { .. } => None,
    }
}

impl Scheduler {
    pub async fn run(self) {
        let mut tick =
            tokio::time::interval(std::time::Duration::from_millis(self.cfg.tick_interval_ms));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut internal = self.events.subscribe_internal();
        let db = self.db.clone();
        let events = self.events.clone();
        tokio::spawn(async move {
            while let Ok(Internal::BotDone {
                bot_id,
                transcript_path,
            }) = internal.recv().await
            {
                if let Err(e) = dispatch::complete_finished_turn(
                    &db,
                    &events,
                    &bot_id,
                    transcript_path.as_deref(),
                ) {
                    tracing::warn!(error = %e, bot_id, "completing routine run failed");
                }
            }
        });

        loop {
            tick.tick().await;
            if let Err(e) = self.step() {
                tracing::warn!(error = %e, "scheduler step failed");
            }
        }
    }

    pub fn step(&self) -> anyhow::Result<()> {
        self.expire_overdue_runs()?;
        self.expire_overdue_tasks()?;
        self.schedule_due_occurrences()?;
        self.execute_due_runs()?;
        Ok(())
    }

    /// A task nobody finished by its deadline dies visibly: both ends get a
    /// note (which invites no reply) instead of the task anchoring reply
    /// chains forever. The open→expired flip gates the notes, so each end is
    /// told exactly once.
    fn expire_overdue_tasks(&self) -> anyhow::Result<()> {
        for task in self.db.overdue_open_tasks(Utc::now())? {
            if !self.db.expire_task(&task.id)? {
                continue;
            }
            tracing::warn!(task_id = %task.id, "task exceeded its deadline");
            let requester = match &task.from_bot_id {
                Some(id) => self.db.get_live_bot(id)?,
                None => None,
            };
            let assignee = self.db.get_bot(&task.to_bot_id)?;
            if let Some(assignee) = assignee.as_ref().filter(|b| b.deleted_at.is_none()) {
                let waiting = match &requester {
                    Some(r) => format!("{} no longer waits on the result", r.name),
                    None => "no one waits on the result".to_string(),
                };
                let body = format!(
                    "Task {} expired: its deadline passed without complete_task. \
                     Stop work on it; {waiting}.",
                    task.id
                );
                self.notify_task_end(&assignee.id, &body, &task.origin_message_id);
            }
            if let Some(requester) = &requester {
                let assignee_name = assignee
                    .as_ref()
                    .map(|b| b.name.clone())
                    .unwrap_or_else(|| task.to_bot_id.clone());
                let body = format!(
                    "Task {} you delegated to {assignee_name} expired without a \
                     result. Re-delegate it with a tighter brief if it still needs \
                     doing.",
                    task.id
                );
                self.notify_task_end(&requester.id, &body, &task.origin_message_id);
            }
        }
        Ok(())
    }

    /// Best-effort expiry note; a failed send must not fail the sweep, or one
    /// archived conversation would wedge every later expiry.
    fn notify_task_end(&self, bot_id: &str, body: &str, ref_message_id: &str) {
        if let Err(e) = crate::messaging::send_dm(
            &self.db,
            &self.events,
            bot_id,
            &crate::messaging::daemon_sender(),
            bus::MessageKind::Note,
            body,
            Some(ref_message_id),
        ) {
            tracing::warn!(error = %e, bot_id, "task expiry note failed");
        }
    }

    /// A lease that lapsed because the daemon died is indistinguishable from
    /// a hung run, and both want the same treatment: retry if attempts remain.
    fn expire_overdue_runs(&self) -> anyhow::Result<()> {
        for run in self.db.overdue_routine_runs()? {
            let Some(routine) = self.db.get_routine(&run.routine_id)? else {
                self.db.finish_running_routine_run(
                    &run.id,
                    RoutineRunState::Failed,
                    Some("routine deleted"),
                )?;
                self.push_run(&run.id)?;
                continue;
            };
            tracing::warn!(run_id = %run.id, routine_id = %routine.id, "routine run exceeded its deadline");
            self.fail_or_retry(&run, &routine, "deadline exceeded")?;
        }
        Ok(())
    }

    /// Occurrences due now, including missed ones inside the catch-up window.
    fn schedule_due_occurrences(&self) -> anyhow::Result<()> {
        let now = Utc::now();
        let window_start = now - Duration::minutes(self.cfg.catch_up_window_minutes);
        for routine in self.db.list_routines(None)? {
            if !routine.enabled || !routine.trigger.is_time_based() {
                continue;
            }
            let mut cursor = window_start;
            // Bounded loop: at most 100 catch-up occurrences per routine/tick.
            for _ in 0..100 {
                match next_occurrence(&routine.trigger, cursor, routine.created_at) {
                    Some(t) if t <= now => {
                        let created = self.db.schedule_routine_run(NewRun {
                            routine_id: &routine.id,
                            scheduled_for: t,
                            source: bus::RunSource::Schedule,
                            signal_id: None,
                        })?;
                        if let Some(run) = created {
                            self.events
                                .push(Push::RoutineRunUpdate { routine_run: run });
                        }
                        cursor = t;
                    }
                    _ => break,
                }
            }
        }
        Ok(())
    }

    fn push_run(&self, run_id: &str) -> anyhow::Result<()> {
        if let Some(run) = self.db.get_routine_run(run_id)? {
            self.events
                .push(Push::RoutineRunUpdate { routine_run: run });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_next_occurrence_aligns_to_anchor() {
        let anchor = Utc::now() - Duration::seconds(95);
        let t = Trigger::Interval { seconds: 60 };
        let next = next_occurrence(&t, Utc::now(), anchor).unwrap();
        let offset = (next - anchor).num_seconds();
        assert_eq!(offset % 60, 0);
        assert!(next > Utc::now());
    }

    #[test]
    fn cron_next_occurrence_parses() {
        let t = Trigger::Cron {
            expr: "0 0 9 * * Mon".to_string(),
            tz: "Europe/Warsaw".to_string(),
        };
        assert!(next_occurrence(&t, Utc::now(), Utc::now()).is_some());
    }

    #[test]
    fn oversized_interval_yields_none_instead_of_panicking() {
        // Values past chrono's duration range used to panic the scheduler
        // task; the unparsable-trigger sentinel (u64::MAX) also lands here.
        for seconds in [u64::MAX, i64::MAX as u64, 10_u64.pow(16)] {
            let t = Trigger::Interval { seconds };
            assert_eq!(next_occurrence(&t, Utc::now(), Utc::now()), None);
        }
    }

    #[test]
    fn signal_trigger_has_no_time_occurrence() {
        let t = Trigger::Signal {
            name: "deploy.finished".to_string(),
            from_bot_id: None,
        };
        assert!(next_occurrence(&t, Utc::now(), Utc::now()).is_none());
        assert!(!t.is_time_based());
    }
}
