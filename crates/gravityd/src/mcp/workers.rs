//! Temporary workers over MCP: `spawn_worker`, `list_workers` and
//! `cancel_worker`. Spawning may wait on a peer, so it runs before the
//! synchronous tool table, as calls bound for a peer do.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::app::AppState;
use crate::botmgmt;
use crate::workers::{self, SpawnRequest, LISTED_FINISHED};

use super::caller;

/// The tool's result when it is `spawn_worker`, or `None` for any other tool.
pub(super) async fn intercept(
    app: &Arc<AppState>,
    bot_id: &str,
    name: &str,
    args: &Value,
) -> Option<anyhow::Result<Value>> {
    (name == "spawn_worker").then_some(())?;
    Some(spawn_worker(app, bot_id, args).await)
}

fn text<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

async fn spawn_worker(app: &Arc<AppState>, bot_id: &str, args: &Value) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    super::selfmgmt::edit_from_args(args, false)?;
    let brief = text(args, "task").ok_or_else(|| anyhow::anyhow!("'task' is required"))?;
    let worker = workers::spawn(
        app,
        &me,
        SpawnRequest {
            brief,
            name: text(args, "name"),
            description: text(args, "description"),
            instructions: text(args, "instructions"),
            runtime: botmgmt::requested_runtime(args)?,
            machine: text(args, "machine"),
            deadline_hours: args.get("deadline_hours").and_then(Value::as_i64),
        },
    )
    .await?;
    let mut out = workers::describe(app, &worker)?;
    out["result"] = json!(match worker.state {
        bus::WorkerState::Running =>
            "Started. Its result arrives as a `done` for the task_id \
             above; the worker is removed once that task closes.",
        bus::WorkerState::Queued =>
            "Queued: every worker slot is busy. It starts by itself \
             when one frees, and its result arrives as a `done` like any task's.",
        _ => "It could not start; see note.",
    });
    Ok(out)
}

pub(super) fn list_workers(app: &Arc<AppState>, bot_id: &str) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let workers = app
        .db
        .workers_of(&me.id, LISTED_FINISHED)?
        .iter()
        .map(|w| workers::describe(app, w))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let running_here = app.db.count_live_workers(&me.project_id)?;
    Ok(json!({
        "workers": workers,
        "running_here": running_here,
        "max_workers_here": app.cfg.max_workers_per_project,
        "queued_in_project": app.db.queued_workers(&me.project_id)?.len(),
    }))
}

pub(super) fn cancel_worker(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let name = text(args, "name").ok_or_else(|| anyhow::anyhow!("'name' is required"))?;
    let reason = text(args, "reason").unwrap_or("");
    let worker = workers::cancel(app, &me, name, reason)?;
    let mut out = workers::describe(app, &worker)?;
    out["result"] = json!("Cancelled. A running worker is told to stop and is removed.");
    Ok(out)
}
