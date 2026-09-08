//! Routine and routine-run requests.

use bus::{OverlapPolicy, RunSource, SignalSource, Trigger};
use serde_json::{json, Value};

use crate::db::NewRun;
use crate::events::Push;
use crate::mcp::validate_trigger;
use crate::routine_validation::{
    checked_limits, checked_routine_fields, checked_signal_payload, run_history_limit,
};
use crate::scheduler::{cancel_run, emit_signal, next_occurrence, EmitSignal};

use super::Conn;

impl Conn {
    // ---- routines ----

    pub(super) fn routine_json(&self, r: &bus::Routine) -> Value {
        let next = next_occurrence(&r.trigger, chrono::Utc::now(), r.created_at);
        let mut v = serde_json::to_value(r).unwrap_or(Value::Null);
        if let (Value::Object(map), Some(n)) = (&mut v, next) {
            if r.enabled {
                map.insert("next_run_at".to_string(), json!(n.to_rfc3339()));
            }
        }
        v
    }

    pub(super) fn list_routines(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let routines: Vec<Value> = self
            .app
            .db
            .list_routines(Some(bot_id))?
            .iter()
            .map(|r| self.routine_json(r))
            .collect();
        self.send(json!({ "type": "routines", "req_id": req_id, "routines": routines }));
        Ok(())
    }

    pub(super) fn create_routine(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let name = Self::str_field(req, "name")?;
        let prompt = Self::str_field(req, "prompt")?;
        checked_routine_fields(Some(name), Some(prompt))?;
        let limits = checked_limits(req)?;
        let trigger: Trigger = serde_json::from_value(
            req.get("trigger")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("'trigger' is required"))?,
        )
        .map_err(|e| anyhow::anyhow!("invalid trigger: {e}"))?;
        let bot = self
            .app
            .db
            .get_bot(bot_id)?
            .ok_or_else(|| anyhow::anyhow!("bot not found"))?;
        validate_trigger(&self.app.db, &bot.project_id, bot_id, &trigger)?;
        let policy = match req.get("overlap_policy") {
            None => OverlapPolicy::Skip,
            Some(value) => value
                .as_str()
                .and_then(OverlapPolicy::parse)
                .ok_or_else(|| anyhow::anyhow!("unknown overlap_policy"))?,
        };
        // User-created routines are approved by construction.
        let routine = self
            .app
            .db
            .create_routine(bot_id, name, &trigger, prompt, policy, true)?;
        let routine = if limits.max_duration_seconds.is_some() || limits.max_attempts.is_some() {
            self.app
                .db
                .update_routine(&routine.id, None, None, None, None, limits)?;
            self.app
                .db
                .get_routine(&routine.id)?
                .ok_or_else(|| anyhow::anyhow!("routine not found"))?
        } else {
            routine
        };
        self.send(
            json!({ "type": "routine", "req_id": req_id, "routine": self.routine_json(&routine) }),
        );
        Ok(())
    }

    pub(super) fn set_routine_enabled(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let routine_id = Self::str_field(req, "routine_id")?;
        let enabled = req
            .get("enabled")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("'enabled' is required"))?;
        self.app.db.set_routine_enabled(routine_id, enabled)?;
        let routine = self
            .app
            .db
            .get_routine(routine_id)?
            .ok_or_else(|| anyhow::anyhow!("routine not found"))?;
        self.send(
            json!({ "type": "routine", "req_id": req_id, "routine": self.routine_json(&routine) }),
        );
        Ok(())
    }

    pub(super) fn run_routine_now(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let routine_id = Self::str_field(req, "routine_id")?;
        let routine = self
            .app
            .db
            .get_routine(routine_id)?
            .ok_or_else(|| anyhow::anyhow!("routine not found"))?;
        let run = self.app.db.schedule_routine_run(NewRun {
            routine_id: &routine.id,
            scheduled_for: chrono::Utc::now(),
            source: RunSource::Manual,
            signal_id: None,
        })?;
        if let Some(run) = run {
            self.app
                .events
                .push(Push::RoutineRunUpdate { routine_run: run });
        }
        self.send(json!({ "type": "ok", "req_id": req_id }));
        Ok(())
    }

    /// Stop an unfinished occurrence, taking its undelivered prompt with it.
    pub(super) fn cancel_routine_run(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let run_id = Self::str_field(req, "routine_run_id")?;
        let run = self
            .app
            .db
            .get_routine_run(run_id)?
            .ok_or_else(|| anyhow::anyhow!("routine run not found"))?;
        if !matches!(
            run.state,
            bus::RoutineRunState::Scheduled | bus::RoutineRunState::Running
        ) {
            anyhow::bail!("run already finished as '{}'", run.state.as_str());
        }
        if !cancel_run(&self.app.db, &run.id, "cancelled from the app")? {
            let state = self
                .app
                .db
                .get_routine_run(run_id)?
                .map(|current| current.state.as_str())
                .unwrap_or("missing");
            anyhow::bail!("run already finished as '{state}'");
        }
        let updated = self
            .app
            .db
            .get_routine_run(run_id)?
            .ok_or_else(|| anyhow::anyhow!("routine run not found"))?;
        self.app.events.push(Push::RoutineRunUpdate {
            routine_run: updated.clone(),
        });
        self.send(json!({ "type": "routine_run", "req_id": req_id, "routine_run": updated }));
        Ok(())
    }

    /// Emit a signal by hand; the seam future producers will use.
    pub(super) fn emit_signal(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let project_id = Self::str_field(req, "project_id")?;
        let name = Self::str_field(req, "name")?;
        let payload = req.get("payload").cloned().unwrap_or(json!({}));
        checked_signal_payload(&payload)?;
        let signal = emit_signal(
            &self.app.db,
            &self.app.events,
            EmitSignal {
                name,
                source: SignalSource::Manual,
                project_id,
                from_bot_id: None,
                payload,
            },
        )?;
        self.send(json!({ "type": "signal", "req_id": req_id, "signal": signal }));
        Ok(())
    }

    pub(super) fn list_routine_runs(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let routine_id = Self::str_field(req, "routine_id")?;
        let limit = run_history_limit(req)?;
        let runs = self.app.db.list_routine_runs(routine_id, limit)?;
        self.send(json!({ "type": "routine_runs", "req_id": req_id, "routine_runs": runs }));
        Ok(())
    }
}
