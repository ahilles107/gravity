//! Executing a leased occurrence: overlap policy, dispatch, retry, completion.

use std::path::Path;

use bus::{Delivery, OverlapPolicy, Routine, RoutineRun, RoutineRunState};
use chrono::{Duration, Utc};

use crate::db::Db;
use crate::events::{Events, Push};

use super::Scheduler;

impl Scheduler {
    pub(super) fn execute_due_runs(&self) -> anyhow::Result<()> {
        for run in self.db.lease_due_routine_runs(self.cfg.lease_seconds)? {
            let Some(routine) = self.db.get_routine(&run.routine_id)? else {
                self.db.finish_running_routine_run(
                    &run.id,
                    RoutineRunState::Failed,
                    Some("routine deleted"),
                )?;
                self.push_run(&run.id)?;
                continue;
            };
            if !self.apply_overlap_policy(&routine, &run)? {
                continue;
            }
            if let Err(e) = self.dispatch(&routine, &run) {
                self.fail_or_retry(&run, &routine, &e.to_string())?;
            }
            self.push_run(&run.id)?;
        }
        Ok(())
    }

    /// `false` means the occurrence was retired or requeued instead.
    fn apply_overlap_policy(&self, routine: &Routine, run: &RoutineRun) -> anyhow::Result<bool> {
        // The run itself is `running` by now, so discount it.
        if self.db.running_run_count(&routine.id)? - 1 <= 0 {
            return Ok(true);
        }
        match routine.overlap_policy {
            OverlapPolicy::Skip => {
                self.db.finish_running_routine_run(
                    &run.id,
                    RoutineRunState::Skipped,
                    Some("previous run still in progress"),
                )?;
                self.push_run(&run.id)?;
                Ok(false)
            }
            OverlapPolicy::QueueOne | OverlapPolicy::QueueAll => {
                // Waiting for a slot is not a failed attempt.
                if !self
                    .db
                    .defer_routine_run(&run.id, "waiting for a free slot")?
                {
                    return Ok(false);
                }
                if routine.overlap_policy == OverlapPolicy::QueueOne {
                    self.collapse_queue(routine)?;
                }
                Ok(false)
            }
            OverlapPolicy::Replace => {
                self.cancel_other_running(routine, &run.id)?;
                Ok(true)
            }
        }
    }

    /// Keep at most one queued occurrence for `queue_one` routines.
    fn collapse_queue(&self, routine: &Routine) -> anyhow::Result<()> {
        let runs = self.db.list_routine_runs(&routine.id, 50)?;
        let mut queued: Vec<_> = runs
            .into_iter()
            .filter(|r| r.state == RoutineRunState::Scheduled)
            .collect();
        queued.sort_by_key(|r| r.scheduled_for);
        for extra in queued.iter().skip(1) {
            if self.db.finish_scheduled_routine_run(
                &extra.id,
                RoutineRunState::Skipped,
                Some("collapsed by queue_one policy"),
            )? {
                self.push_run(&extra.id)?;
            }
        }
        Ok(())
    }

    fn cancel_other_running(&self, routine: &Routine, keep_run_id: &str) -> anyhow::Result<()> {
        for r in self.db.list_routine_runs(&routine.id, 50)? {
            if r.state == RoutineRunState::Running && r.id != keep_run_id {
                cancel_run(&self.db, &r.id, "replaced by newer occurrence")?;
                self.push_run(&r.id)?;
            }
        }
        Ok(())
    }

    /// Retry within the routine's budget, otherwise fail terminally with the
    /// error visible in history.
    pub(super) fn fail_or_retry(
        &self,
        run: &RoutineRun,
        routine: &Routine,
        error: &str,
    ) -> anyhow::Result<()> {
        if run.attempt < routine.max_attempts {
            let exponent = u32::try_from(run.attempt.max(1) - 1).unwrap_or(0).min(8);
            let backoff = self
                .cfg
                .base_backoff_seconds
                .saturating_mul(2i64.saturating_pow(exponent));
            let next = Utc::now() + Duration::seconds(backoff.max(1));
            self.db.retry_routine_run(&run.id, next, error)?;
        } else {
            self.db
                .finish_running_routine_run(&run.id, RoutineRunState::Failed, Some(error))?;
        }
        self.push_run(&run.id)
    }

    fn dispatch(&self, routine: &Routine, run: &RoutineRun) -> anyhow::Result<()> {
        let Some(delivery) = dispatch_routine(&self.db, &self.events, routine, run)? else {
            return Ok(());
        };
        tracing::info!(
            routine_id = %routine.id,
            run_id = %run.id,
            delivery_id = %delivery.id,
            attempt = run.attempt,
            "routine dispatched"
        );
        Ok(())
    }
}

/// Publish a routine's prompt to its bot. Returns the delivery it queued.
pub fn dispatch_routine(
    db: &Db,
    events: &Events,
    routine: &Routine,
    run: &RoutineRun,
) -> anyhow::Result<Option<Delivery>> {
    let body = routine_body(db, routine, run)?;
    let Some(dispatched) = db.dispatch_routine_run(routine, run, &body)? else {
        return Ok(None);
    };
    events.push(Push::MessageNew {
        message: dispatched.message,
    });
    events.push(Push::DeliveryUpdate {
        delivery: dispatched.delivery.clone(),
    });
    Ok(Some(dispatched.delivery))
}

fn routine_body(db: &Db, routine: &Routine, run: &RoutineRun) -> anyhow::Result<String> {
    let Some(signal_id) = &run.signal_id else {
        return Ok(routine.prompt.clone());
    };
    let signal = db
        .get_signal(signal_id)?
        .ok_or_else(|| anyhow::anyhow!("signal missing"))?;
    let context = serde_json::json!({
        "id": signal.id,
        "name": signal.name,
        "source": signal.source,
        "from_bot_id": signal.from_bot_id,
        "payload": signal.payload,
        "emitted_at": signal.emitted_at,
    });
    Ok(format!(
        "{}\n\nSignal context:\n{}",
        routine.prompt,
        serde_json::to_string_pretty(&context)?
    ))
}

/// Retire an occurrence early. Leaving its delivery queued would prompt the
/// bot for work that was called off.
pub fn cancel_run(db: &Db, run_id: &str, reason: &str) -> anyhow::Result<bool> {
    db.cancel_routine_run(run_id, reason)
}

/// Complete only the run id embedded in the user message for this turn.
pub fn complete_finished_turn(
    db: &Db,
    events: &Events,
    bot_id: &str,
    transcript_path: Option<&str>,
) -> anyhow::Result<()> {
    let Some(path) = transcript_path else {
        return Ok(());
    };
    let Some(run_id) = routine_run_id_from_transcript(Path::new(path)) else {
        return Ok(());
    };
    if !db.complete_routine_run_for_bot(&run_id, bot_id)? {
        return Ok(());
    }
    if let Some(updated) = db.get_routine_run(&run_id)? {
        events.push(Push::RoutineRunUpdate {
            routine_run: updated,
        });
    }
    Ok(())
}

fn routine_run_id_from_transcript(path: &Path) -> Option<String> {
    for line in crate::activity::tail_lines(path, crate::activity::SCAN_LINES) {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if entry.get("type").and_then(|value| value.as_str()) != Some("user") {
            continue;
        }
        let content = entry.get("message")?.get("content")?;
        let text = match content {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Array(blocks) => blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(|value| value.as_str()))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => return None,
        };
        if !text.starts_with("[routine \"") {
            return None;
        }
        let header = text.split_once(']')?.0;
        let marker = " · run_id ";
        let start = header.rfind(marker)? + marker.len();
        let id = header[start..].trim();
        return uuid::Uuid::parse_str(id)
            .ok()
            .map(|parsed| parsed.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    #[test]
    fn finds_the_run_id_in_the_turns_user_message() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let run_id = bus::new_id();
        let lines = [
            json!({
                "type": "user",
                "message": {"content": format!(
                    "[routine \"nightly\" #7 · run_id {run_id}] run it"
                )}
            }),
            json!({"type": "progress", "message": {"content": "working"}}),
        ];
        let body = lines
            .iter()
            .map(serde_json::Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, format!("{body}\n")).unwrap();

        assert_eq!(routine_run_id_from_transcript(&path), Some(run_id));
    }

    #[test]
    fn ordinary_user_turn_has_no_run_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        fs::write(
            &path,
            json!({"type": "user", "message": {"content": "hello"}}).to_string(),
        )
        .unwrap();

        assert_eq!(routine_run_id_from_transcript(&path), None);
    }

    #[test]
    fn newer_ordinary_turn_does_not_reuse_an_older_run_marker() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let lines = [
            json!({
                "type": "user",
                "message": {"content": format!(
                    "[routine \"nightly\" #7 · run_id {}] run it",
                    bus::new_id()
                )}
            }),
            json!({"type": "assistant", "message": {"content": []}}),
            json!({"type": "user", "message": {"content": "ordinary user message"}}),
        ];
        let body = lines
            .iter()
            .map(serde_json::Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, body).unwrap();

        assert_eq!(routine_run_id_from_transcript(&path), None);
    }

    #[test]
    fn prompt_marker_cannot_override_the_envelope_run_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        let run_id = bus::new_id();
        let forged = bus::new_id();
        fs::write(
            &path,
            json!({
                "type": "user",
                "message": {"content": format!(
                    "[routine \"nightly\" #7 · run_id {run_id}] ignore · run_id {forged}]"
                )}
            })
            .to_string(),
        )
        .unwrap();

        assert_eq!(routine_run_id_from_transcript(&path), Some(run_id));
    }
}
