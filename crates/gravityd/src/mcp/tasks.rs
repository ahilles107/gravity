//! Closing a task: the assignee's `complete_task` and the requester's
//! `cancel_task`.

use std::sync::Arc;

use bus::{MessageKind, TaskState, MAX_MESSAGE_BYTES};
use serde_json::{json, Value};

use crate::app::AppState;
use crate::events::Push;
use crate::messaging;

use super::{bot_sender, caller};

pub(super) fn complete_task(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let task_id = args
        .get("task_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("'task_id' is required"))?;
    let result = args
        .get("result")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("'result' is required"))?;
    if result.len() > MAX_MESSAGE_BYTES {
        anyhow::bail!("result exceeds {MAX_MESSAGE_BYTES} bytes");
    }
    // Artifact paths ride in the done body so the requester reads the files
    // instead of a lossy retelling.
    let artifacts: Vec<&str> = args
        .get("artifacts")
        .and_then(|v| v.as_array())
        .map(|paths| paths.iter().filter_map(|p| p.as_str()).collect())
        .unwrap_or_default();
    let result = if artifacts.is_empty() {
        result.to_string()
    } else {
        let listing: String = artifacts.iter().map(|p| format!("\n- {p}")).collect();
        format!("{result}\n\nartifacts:{listing}")
    };
    let task = app
        .db
        .get_task(task_id)?
        .ok_or_else(|| anyhow::anyhow!("task not found"))?;
    if task.to_bot_id != me.id {
        anyhow::bail!("task {task_id} is not assigned to you");
    }
    // The flip is the gate: it both rejects an already-closed task and stops a
    // concurrent cancel_task from overwriting this result.
    if !app.db.try_close_task(task_id, TaskState::Done)? {
        anyhow::bail!(
            "task is already {}",
            current_task_state(app, task_id, task.state)
        );
    }

    let origin = app
        .db
        .get_message(&task.origin_message_id)?
        .ok_or_else(|| anyhow::anyhow!("origin message missing"))?;

    // Publish the result where the task originated: the origin conversation.
    let msg = app.db.insert_message(
        &origin.conversation_id,
        &bot_sender(&me),
        MessageKind::Done,
        &result,
        Some(&origin.id),
    )?;
    app.events.push(Push::MessageNew {
        message: msg.clone(),
    });
    // Deliver the completion to the requesting bot, if a bot asked.
    if let Some(from_bot) = &task.from_bot_id {
        let key = format!("{}:{}", msg.id, from_bot);
        let delivery = app.db.enqueue_delivery(&msg.id, from_bot, &key)?;
        app.events.push(Push::DeliveryUpdate { delivery });
    }
    Ok(json!({ "message_id": msg.id, "num": msg.num }))
}

/// Close an open task you delegated. `complete_task` is the assignee's end of
/// a task; this is the requester's — the way out of a task that will never
/// come back because the bot holding it is stuck, gone, or no longer needed.
/// Cancelling frees the fan-out slot the task holds.
pub(super) fn cancel_task(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let task_id = args
        .get("task_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("'task_id' is required"))?;
    let reason = args.get("reason").and_then(|v| v.as_str()).unwrap_or("");
    let task = app
        .db
        .get_task(task_id)?
        .ok_or_else(|| anyhow::anyhow!("task not found"))?;
    if task.from_bot_id.as_deref() != Some(me.id.as_str()) {
        anyhow::bail!(
            "task {task_id} is not yours to cancel — only the bot that delegated \
             it can. If it was delegated to you, report what you have with \
             complete_task"
        );
    }

    // The assignee may still be working, so it is told to stop. A note invites
    // no reply, which is right here: nothing it sends back can reopen the task.
    // It carries the canceller's own name, never the daemon's — the envelope
    // header is an authenticity claim, and `reason` is one bot's words.
    let because = if reason.is_empty() {
        String::new()
    } else {
        format!(" Reason: {reason}")
    };
    let body = format!(
        "Task {task_id} was cancelled. Stop work on it; no one waits on the \
         result.{because}"
    );
    // Before the flip: an oversized reason leaves the task open, not closed
    // with nobody told.
    if body.len() > MAX_MESSAGE_BYTES {
        anyhow::bail!("reason exceeds {MAX_MESSAGE_BYTES} bytes");
    }
    if !app.db.try_close_task(task_id, TaskState::Cancelled)? {
        anyhow::bail!(
            "task is already {}",
            current_task_state(app, task_id, task.state)
        );
    }

    let mut notified = false;
    if let Some(assignee) = app.db.get_live_bot(&task.to_bot_id)? {
        messaging::send_dm(
            &app.db,
            &app.events,
            &assignee.id,
            &bot_sender(&me),
            MessageKind::Note,
            &body,
            Some(&task.origin_message_id),
        )?;
        notified = true;
    }
    Ok(json!({
        "task_id": task_id,
        "state": TaskState::Cancelled.as_str(),
        "notified": notified
    }))
}

/// A task's state as stored, for an error message written after a failed
/// close. Falls back to what the caller read when the row cannot be re-read.
fn current_task_state(app: &Arc<AppState>, task_id: &str, fallback: TaskState) -> &'static str {
    app.db
        .get_task(task_id)
        .ok()
        .flatten()
        .map_or(fallback, |t| t.state)
        .as_str()
}

/// Render tasks as `<id> (to <bot>)`, so a refusal names what to act on.
pub(super) fn describe_tasks(app: &Arc<AppState>, tasks: &[bus::Task]) -> anyhow::Result<String> {
    let mut parts = Vec::with_capacity(tasks.len());
    for task in tasks {
        let to = app
            .db
            .get_bot(&task.to_bot_id)?
            .map(|b| b.name)
            .unwrap_or_else(|| task.to_bot_id.clone());
        parts.push(format!("{} (to {to})", task.id));
    }
    Ok(parts.join(", "))
}
