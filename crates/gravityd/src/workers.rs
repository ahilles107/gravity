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

use std::collections::HashSet;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use bus::{TaskState, Worker, WorkerState};
use serde_json::{json, Value};
use tokio::sync::{Mutex, Notify};

use crate::app::AppState;
use crate::botmgmt;
use crate::db::Actor;
use crate::mcp::tasks::close_cancelled;
use crate::messaging::daemon_sender;

mod git;
mod place;
pub mod repo;
mod spawn;

pub use place::place_queued;
pub use spawn::{cancel, cancel_spawn, spawn, SpawnRequest, MAX_QUEUED_PER_PROJECT};

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
    /// One placement pass here at a time, or two would hand out the same slot.
    placing: Mutex<()>,
    /// Spawns being offered to linked machines, which another pass leaves
    /// alone until the offer is answered.
    offering: StdMutex<HashSet<String>>,
    /// Retiring workers whose unpushed work is being saved.
    salvaging: StdMutex<HashSet<String>>,
}

impl Workers {
    /// Reconcile soon: a task closed, a worker was asked for, a slot freed.
    pub fn nudge(&self) {
        self.wake.notify_one();
    }

    fn set(set: &StdMutex<HashSet<String>>) -> std::sync::MutexGuard<'_, HashSet<String>> {
        set.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn is_offering(&self, worker_id: &str) -> bool {
        Self::set(&self.offering).contains(worker_id)
    }

    fn start_offering(&self, worker_id: &str) {
        Self::set(&self.offering).insert(worker_id.to_string());
    }

    fn stop_offering(&self, worker_id: &str) {
        Self::set(&self.offering).remove(worker_id);
    }
}

/// Written to a retiring worker's workspace once its unpushed work has been
/// saved, or found to need none, so a later pass does not try again.
const SALVAGED_MARKER: &str = ".gravity-salvaged";

/// Tell clients a project's queue changed.
pub(crate) fn changed(app: &AppState, project_id: &str) {
    app.events.push(crate::events::Push::WorkersUpdated {
        project_id: project_id.to_string(),
    });
}

/// A spawn as the app lists it: what a parent reads, plus who asked, the
/// brief's opening, and when it moved.
pub fn view(app: &AppState, worker: &Worker) -> anyhow::Result<Value> {
    let mut out = describe(app, worker)?;
    let parent = app.db.get_bot(&worker.parent_bot_id)?;
    out["id"] = json!(worker.id);
    out["project_id"] = json!(worker.project_id);
    out["parent_bot_id"] = json!(worker.parent_bot_id);
    out["parent_name"] = json!(parent.as_ref().map(crate::db::Db::display_name));
    out["brief"] = json!(worker.brief.chars().take(500).collect::<String>());
    out["bot_id"] = json!(worker.bot_id);
    out["created_at"] = json!(worker.created_at.to_rfc3339());
    out["started_at"] = json!(worker.started_at.map(|t| t.to_rfc3339()));
    out["finished_at"] = json!(worker.finished_at.map(|t| t.to_rfc3339()));
    Ok(out)
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
        // Unpushed work is saved first, in the background, and the worker
        // retires on a later pass once its note has left — so it never
        // speaks after it is archived, and a slow push holds up nothing.
        if !salvaged(app, &bot) {
            continue;
        }
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

/// Whether a retiring worker's work is safe: it has no checkout, or saving
/// what it left unpushed has finished. Otherwise saving is started, or is
/// still running, and the worker waits.
fn salvaged(app: &Arc<AppState>, bot: &bus::Bot) -> bool {
    let workspace = std::path::PathBuf::from(&bot.workspace_path);
    let marker = workspace.join(SALVAGED_MARKER);
    let Some(checkout) = repo::checkout_of(&workspace) else {
        return true;
    };
    if marker.exists() {
        return true;
    }
    if !Workers::set(&app.workers.salvaging).insert(bot.id.clone()) {
        return false;
    }
    let (app, bot) = (app.clone(), bot.clone());
    tokio::spawn(async move {
        let (dir, name, id) = (checkout.clone(), bot.name.clone(), bot.id.clone());
        let saved = tokio::task::spawn_blocking(move || repo::salvage(&dir, &name, &id))
            .await
            .unwrap_or_else(|error| repo::Salvaged::Failed(error.to_string()));
        if let Some(line) = saved.report(&checkout) {
            tell_parent(&app, &bot, &line);
        }
        if let Err(error) = std::fs::write(&marker, "") {
            tracing::warn!(bot_id = %bot.id, %error, "could not mark a worker salvaged");
        }
        Workers::set(&app.workers.salvaging).remove(&bot.id);
        app.workers.nudge();
    });
    false
}

/// Tell whoever gave a retiring worker its task where its work went, in the
/// worker's own name.
fn tell_parent(app: &Arc<AppState>, bot: &bus::Bot, line: &str) {
    let Ok(Some(task)) = app.db.latest_task_to(&bot.id) else {
        return;
    };
    let Some(parent) = task
        .from_bot_id
        .as_deref()
        .and_then(|id| app.db.get_live_bot(id).ok().flatten())
    else {
        return;
    };
    let ended = match task.state {
        TaskState::Done => "finished".to_string(),
        state => format!("stopped: its task was {}", state.as_str()),
    };
    let body = format!("{} {ended}. {line}", bot.name);
    let sender = crate::mcp::bot_sender(bot);
    if let Err(error) = crate::messaging::send_dm(
        &app.db,
        &app.events,
        crate::messaging::Dm::new(&parent.id, &sender, bus::MessageKind::Note, &body)
            .re(&task.origin_message_id),
    ) {
        tracing::warn!(bot_id = %bot.id, %error, "could not tell the parent about saved work");
    }
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
    if app
        .db
        .finish_worker(&worker.id, WorkerState::Cancelled, Some(reason))?
    {
        changed(app, &worker.project_id);
    }
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
    if app.db.finish_worker(&worker.id, state, None)? {
        changed(app, &worker.project_id);
    }
    Ok(())
}
