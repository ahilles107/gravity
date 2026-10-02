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
    /// One placement pass at a time, or two would hand out the same slot.
    placing: Mutex<()>,
}

impl Workers {
    /// Reconcile soon: a task closed, a worker was asked for, a slot freed.
    pub fn nudge(&self) {
        self.wake.notify_one();
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
        // A note about saved work went out: retire once it has left, which a
        // later pass sees, so the worker never speaks after it is archived.
        if salvage(app, &bot).await {
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
            continue;
        }
        let workspace = std::path::PathBuf::from(&bot.workspace_path);
        tokio::task::spawn_blocking(move || repo::retire(&workspace))
            .await
            .ok();
    }
    for project_id in app.db.projects_with_queued_workers()? {
        place_queued(app, &project_id).await;
    }
    Ok(())
}

/// Push work a worker never got onto the shared branch — its task was
/// cancelled or expired, or its push failed — to its own branch, and tell
/// whoever spawned it where that is. True when a note was sent.
async fn salvage(app: &Arc<AppState>, bot: &bus::Bot) -> bool {
    let workspace = std::path::PathBuf::from(&bot.workspace_path);
    let marker = workspace.join(SALVAGED_MARKER);
    let Some(checkout) = repo::checkout_of(&workspace).filter(|_| !marker.exists()) else {
        return false;
    };
    let (dir, name) = (checkout.clone(), bot.name.clone());
    let saved = tokio::task::spawn_blocking(move || repo::salvage(&dir, &name))
        .await
        .unwrap_or_else(|error| repo::Salvaged::Failed(error.to_string()));
    if let Err(error) = std::fs::write(&marker, "") {
        tracing::warn!(bot_id = %bot.id, %error, "could not mark a worker salvaged");
    }
    let Some(line) = saved.report(&checkout) else {
        return false;
    };
    let Ok(Some(task)) = app.db.latest_task_to(&bot.id) else {
        return false;
    };
    let Some(parent) = task
        .from_bot_id
        .as_deref()
        .and_then(|id| app.db.get_live_bot(id).ok().flatten())
    else {
        return false;
    };
    let ended = match task.state {
        TaskState::Done => "finished".to_string(),
        state => format!("stopped: its task was {}", state.as_str()),
    };
    let body = format!("{} {ended}. {line}", bot.name);
    let sender = crate::mcp::bot_sender(bot);
    let sent = crate::messaging::send_dm(
        &app.db,
        &app.events,
        crate::messaging::Dm::new(&parent.id, &sender, bus::MessageKind::Note, &body)
            .re(&task.origin_message_id),
    );
    if let Err(error) = &sent {
        tracing::warn!(bot_id = %bot.id, %error, "could not tell the parent about saved work");
    }
    sent.is_ok()
}

/// Give a just-created worker its checkout of the project's repository, or
/// retire it again when that fails: a worker without its checkout would work
/// from nothing.
pub fn check_out_or_retire(
    app: &Arc<AppState>,
    bot: &bus::Bot,
    repo: &bus::ProjectRepo,
) -> anyhow::Result<std::path::PathBuf> {
    let project = app
        .db
        .get_project(&bot.project_id)?
        .ok_or_else(|| anyhow::anyhow!("project not found"))?;
    let root = crate::paths::project_dir(&app.cfg, &project.dir_name);
    let workspace = std::path::Path::new(&bot.workspace_path);
    repo::check_out(&root, repo, workspace, &bot.id, &bot.name).or_else(|error| {
        botmgmt::archive_bot(app, bot, &Actor::User, Some("its checkout failed"))?;
        Err(error.context(format!("checking out {} for {}", repo.url, bot.name)))
    })
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
