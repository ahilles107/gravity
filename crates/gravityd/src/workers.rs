//! Temporary workers: bots a bot spawns for one task each.
//!
//! A spawn is queued first and placed when a worker slot is free — on this
//! machine, or on a machine the project is linked with. Placing it creates a
//! temporary bot and delegates the brief to it as an ordinary task, so its
//! result comes back to the parent as any task's `done` does. Once that task
//! closes, the worker is archived and its slot goes to the next spawn.
//!
//! Workers have a cap of their own, `max_workers_per_project` per machine,
//! so a project whose permanent bots fill the bot cap can still fan out. The
//! queue is what lets a parent ask for more than fits: spawns past the cap
//! wait, oldest first, and start as earlier ones finish.

use std::sync::Arc;
use std::time::Duration;

use bus::{TaskState, Worker, WorkerState};
use serde_json::{json, Value};
use tokio::sync::{Mutex, Notify};

use crate::app::AppState;
use crate::botmgmt;
use crate::db::Actor;
use crate::mcp::tasks::close_cancelled;
use crate::messaging::daemon_sender;

mod place;
mod spawn;

pub use place::place_queued;
pub use spawn::{cancel, spawn, SpawnRequest, MAX_QUEUED_PER_PROJECT};

/// How often workers are reconciled when nothing nudges sooner. Bounds how
/// long a slot freed on a peer, or by an expired task, sits unused.
const RECONCILE_INTERVAL: Duration = Duration::from_secs(5);

/// Finished spawns `list_workers` shows beside the active ones.
pub const LISTED_FINISHED: i64 = 20;

/// `machine` value pinning a spawn to this daemon.
pub const HERE: &str = "here";

#[derive(Default)]
pub struct Workers {
    wake: Notify,
    /// One placement pass at a time, or two would hand out the same slot.
    placing: Mutex<()>,
}

impl Workers {
    /// Reconcile soon: a task closed, a worker was asked for, a slot freed.
    pub fn nudge(&self) {
        self.wake.notify_one();
    }
}

/// A spawn as a parent reads it in tool results.
pub fn describe(app: &AppState, worker: &Worker) -> anyhow::Result<Value> {
    let mut out = json!({
        "name": worker.name,
        "state": worker.state.as_str(),
    });
    if let Some(position) = app.db.queue_position(worker)? {
        out["queue_position"] = json!(position);
    }
    if let Some(bot) = worker
        .bot_id
        .as_deref()
        .map(|id| app.db.get_bot(id))
        .transpose()?
        .flatten()
    {
        let machine = match bot.peer_id.as_deref() {
            Some(peer) => app.db.get_peer(peer)?.map(|p| p.name),
            None => Some(HERE.to_string()),
        };
        out["machine"] = json!(machine);
    } else if let Some(machine) = &worker.machine {
        out["machine"] = json!(machine);
    }
    if let Some(task) = &worker.task_id {
        out["task_id"] = json!(task);
    }
    if let Some(error) = &worker.error {
        out["note"] = json!(error);
    }
    Ok(out)
}

/// Keep workers moving until the daemon stops.
pub async fn run(app: Arc<AppState>) {
    loop {
        if let Err(error) = reconcile(&app).await {
            tracing::warn!(%error, "worker reconciliation failed");
        }
        tokio::select! {
            () = app.workers.wake.notified() => {}
            () = tokio::time::sleep(RECONCILE_INTERVAL) => {}
        }
    }
}

/// One pass: settle spawns whose task closed, retire finished workers, and
/// hand freed slots to the queue.
pub async fn reconcile(app: &Arc<AppState>) -> anyhow::Result<()> {
    for worker in app.db.orphaned_workers()? {
        release_orphan(app, &worker)?;
    }
    for worker in app.db.running_workers()? {
        settle(app, &worker)?;
    }
    for bot in app.db.finished_temporary_bots(chrono::Utc::now())? {
        let actor = match &bot.created_by_bot_id {
            Some(creator) => Actor::Bot {
                id: creator,
                project_id: &bot.project_id,
            },
            None => Actor::User,
        };
        if let Err(error) = botmgmt::archive_bot(app, &bot, &actor, Some("worker finished")) {
            tracing::warn!(bot_id = %bot.id, %error, "retiring a worker failed");
        }
    }
    for project_id in app.db.projects_with_queued_workers()? {
        place_queued(app, &project_id).await;
    }
    Ok(())
}

/// A spawn whose parent was deleted: nobody waits on it any more.
pub(super) fn release_orphan(app: &Arc<AppState>, worker: &Worker) -> anyhow::Result<()> {
    let reason = "the bot that spawned it was deleted";
    if let Some(task) = worker
        .task_id
        .as_deref()
        .map(|id| app.db.get_task(id))
        .transpose()?
        .flatten()
    {
        let body = format!(
            "Task {} was cancelled: the bot that spawned you was deleted. Stop work on it.",
            task.id
        );
        close_cancelled(app, &daemon_sender(), &task, &body)?;
    }
    app.db
        .finish_worker(&worker.id, WorkerState::Cancelled, Some(reason))?;
    Ok(())
}

/// Close a running spawn whose task has closed, with the task's outcome.
fn settle(app: &Arc<AppState>, worker: &Worker) -> anyhow::Result<()> {
    let task = match worker.task_id.as_deref() {
        Some(id) => app.db.get_task(id)?,
        None => None,
    };
    let state = match task.map(|t| t.state) {
        Some(TaskState::Open) => return Ok(()),
        Some(TaskState::Done) => WorkerState::Done,
        Some(TaskState::Cancelled) => WorkerState::Cancelled,
        Some(TaskState::Expired) => WorkerState::Expired,
        None => WorkerState::Failed,
    };
    app.db.finish_worker(&worker.id, state, None)?;
    Ok(())
}
