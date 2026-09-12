//! Routine and signal tools: a bot managing its own schedule, and announcing
//! things other bots' routines react to.

use std::sync::Arc;

use bus::{OverlapPolicy, Trigger};
use serde_json::{json, Value};

use crate::app::AppState;
use crate::events::Push;
use crate::routine_validation::{checked_limits, checked_routine_fields, checked_signal_payload};
use crate::scheduler::{emit_signal as emit, EmitSignal};

use super::{caller, validate_trigger};

pub(super) fn create_routine(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("'name' is required"))?;
    let prompt = args
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("'prompt' is required"))?;
    let trigger: Trigger = serde_json::from_value(
        args.get("trigger")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("'trigger' is required"))?,
    )
    .map_err(|e| anyhow::anyhow!("invalid trigger: {e}"))?;
    checked_routine_fields(Some(name), Some(prompt))?;
    let limits = checked_limits(args)?;
    validate_trigger(&app.db, &me.project_id, &me.id, &trigger)?;
    let policy = args
        .get("busy_policy")
        .and_then(|v| v.as_str())
        .and_then(OverlapPolicy::parse)
        .unwrap_or(OverlapPolicy::Skip);
    // Routines run as soon as they are created. Gating these while bot
    // creation is ungated would be incoherent; the 60-second floor that
    // `validate_trigger` holds for interval and cron triggers alike is what
    // bounds scheduling pressure.
    let routine = app
        .db
        .create_routine(&me.id, name, &trigger, prompt, policy, true)?;
    if limits.max_duration_seconds.is_some() || limits.max_attempts.is_some() {
        app.db
            .update_routine(&routine.id, None, None, None, None, limits)?;
    }
    app.events.push(Push::notice(
        "info",
        "Routine created",
        format!("Bot {} scheduled routine \"{}\".", me.name, name),
    ));
    Ok(json!({ "id": routine.id, "enabled": true }))
}

pub(super) fn list_routines(app: &Arc<AppState>, bot_id: &str) -> anyhow::Result<Value> {
    let routines = app.db.list_routines(Some(bot_id))?;
    Ok(serde_json::to_value(json!({ "routines": routines }))?)
}

/// Resolve a routine the caller owns; bots may only touch their own routines.
fn my_routine(app: &Arc<AppState>, bot_id: &str, args: &Value) -> anyhow::Result<bus::Routine> {
    let routine_id = args
        .get("routine_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("'routine_id' is required"))?;
    let routine = app
        .db
        .get_routine(routine_id)?
        .ok_or_else(|| anyhow::anyhow!("routine not found"))?;
    if routine.bot_id != bot_id {
        anyhow::bail!("routine belongs to another bot");
    }
    Ok(routine)
}

pub(super) fn set_routine_enabled(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let routine = my_routine(app, bot_id, args)?;
    let enabled = args
        .get("enabled")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| anyhow::anyhow!("'enabled' is required"))?;
    app.db.set_routine_enabled(&routine.id, enabled)?;
    Ok(json!({ "id": routine.id, "enabled": enabled }))
}

pub(super) fn update_routine(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let routine = my_routine(app, bot_id, args)?;
    let name = args.get("name").and_then(|v| v.as_str());
    let prompt = args.get("prompt").and_then(|v| v.as_str());
    checked_routine_fields(name, prompt)?;
    let trigger: Option<Trigger> = args
        .get("trigger")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| anyhow::anyhow!("invalid trigger: {e}"))?;
    if let Some(t) = &trigger {
        validate_trigger(&app.db, &me.project_id, &me.id, t)?;
    }
    // Unlike create_routine, an explicit but unknown policy is an error here:
    // silently keeping the old value would look like the edit was applied.
    let policy = match args.get("busy_policy").and_then(|v| v.as_str()) {
        Some(s) => Some(
            OverlapPolicy::parse(s).ok_or_else(|| anyhow::anyhow!("unknown busy_policy: {s}"))?,
        ),
        None => None,
    };
    let limits = checked_limits(args)?;
    let nothing = name.is_none()
        && prompt.is_none()
        && trigger.is_none()
        && policy.is_none()
        && limits.max_duration_seconds.is_none()
        && limits.max_attempts.is_none();
    if nothing {
        anyhow::bail!(
            "nothing to update: pass name, trigger, prompt, busy_policy, \
             max_duration_seconds or max_attempts"
        );
    }
    app.db
        .update_routine(&routine.id, name, trigger.as_ref(), prompt, policy, limits)?;
    // Queued occurrences were computed from the old trigger; drop them so the
    // new schedule starts clean instead of firing at stale times.
    if trigger.is_some() {
        app.db
            .cancel_scheduled_runs(&routine.id, "trigger changed")?;
    }
    let updated = app
        .db
        .get_routine(&routine.id)?
        .ok_or_else(|| anyhow::anyhow!("routine not found"))?;
    Ok(serde_json::to_value(updated)?)
}

pub(super) fn delete_routine(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let routine = my_routine(app, bot_id, args)?;
    app.db.delete_routine(&routine.id)?;
    app.events.push(Push::notice(
        "info",
        "Routine deleted",
        format!("Bot {} deleted routine \"{}\".", me.name, routine.name),
    ));
    Ok(json!({ "deleted": routine.name }))
}

/// The deliberate replacement for event triggers: a bot names what it is
/// announcing, instead of every turn it ends waking every routine watching it.
pub(super) fn emit_signal(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("'name' is required"))?;
    let payload = args.get("payload").cloned().unwrap_or(json!({}));
    checked_signal_payload(&payload)?;
    let signal = emit(
        &app.db,
        &app.events,
        EmitSignal {
            name,
            source: bus::SignalSource::Bot,
            project_id: &me.project_id,
            from_bot_id: Some(&me.id),
            payload,
        },
    )?;
    Ok(json!({ "id": signal.id, "name": signal.name, "hop_count": signal.hop_count }))
}
