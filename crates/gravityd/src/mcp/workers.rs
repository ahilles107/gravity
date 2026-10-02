//! Temporary workers over MCP: `spawn_worker`, `list_workers` and
//! `cancel_worker`, and a worker's `complete_task`, which first pushes its
//! work to the project's repository. Spawning may wait on a peer and pushing
//! on the network, so both run before the synchronous tool table, as calls
//! bound for a peer do.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::app::AppState;
use crate::botmgmt;
use crate::workers::{self, repo, SpawnRequest, LISTED_FINISHED};

use super::caller;

/// The tool's result when it is one handled here, or `None` otherwise.
pub(super) async fn intercept(
    app: &Arc<AppState>,
    bot_id: &str,
    name: &str,
    args: &Value,
) -> Option<anyhow::Result<Value>> {
    match name {
        "spawn_worker" => Some(spawn_worker(app, bot_id, args).await),
        "complete_task" => complete_with_push(app, bot_id, args).await,
        _ => None,
    }
}

/// A worker with a checkout pushes its work before its result goes out, so
/// the parent can pull it the moment it reads the `done`. The result says
/// where the work went.
async fn complete_with_push(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> Option<anyhow::Result<Value>> {
    let me = caller(app, bot_id).ok().filter(|b| b.temporary)?;
    let checkout = repo::checkout_of(std::path::Path::new(&me.workspace_path))?;
    // Only for the task it holds, so a mistyped id pushes nothing.
    let task_id = text(args, "task_id")?;
    let task = app.db.get_task(task_id).ok().flatten()?;
    if task.to_bot_id != me.id || task.state != bus::TaskState::Open {
        return None;
    }
    let result = text(args, "result").unwrap_or_default().to_string();
    let (name, summary, dir) = (me.name.clone(), result.clone(), checkout.clone());
    let published = tokio::task::spawn_blocking(move || repo::publish(&dir, &name, &summary))
        .await
        .unwrap_or_else(|error| repo::Published::Failed(error.to_string()));
    let mut args = args.clone();
    args["result"] = json!(format!("{result}\n\n{}", published.report(&checkout)));
    Some(super::tasks::complete_task(app, bot_id, &args))
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
