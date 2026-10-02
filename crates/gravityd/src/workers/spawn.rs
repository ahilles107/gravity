//! Asking for workers: a spawn's validation and queueing, and cancelling one.

use std::sync::Arc;

use anyhow::bail;
use bus::{Bot, BotRuntime, Worker, WorkerState};

use crate::app::AppState;
use crate::botmgmt;
use crate::db::NewWorker;
use crate::mcp::bot_sender;
use crate::mcp::tasks::close_cancelled;

use super::place::{delegation_chain, linked_peer_named};
use super::{place_queued, HERE};

/// Spawns one project may hold in its queue, so a runaway loop cannot fill
/// the database with work that will never run.
pub const MAX_QUEUED_PER_PROJECT: usize = 200;

/// The longest a worker's task may run before it expires, in hours.
const MAX_DEADLINE_HOURS: i64 = 168;

/// A spawn as a parent asks for it.
pub struct SpawnRequest<'a> {
    pub brief: &'a str,
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub instructions: Option<&'a str>,
    pub runtime: Option<BotRuntime>,
    /// [`HERE`], a peer's name, or `None` for any machine with a free slot.
    pub machine: Option<&'a str>,
    pub deadline_hours: Option<i64>,
}

/// Queue a spawn and try to place it at once. Returns the spawn as it stands
/// afterwards: `running` when a slot was free, `queued` otherwise.
pub async fn spawn(
    app: &Arc<AppState>,
    parent: &Bot,
    req: SpawnRequest<'_>,
) -> anyhow::Result<Worker> {
    // Workers spawning workers could fill every slot with parents waiting on
    // children that can never be placed.
    if parent.temporary {
        bail!(
            "workers cannot spawn workers — do the work yourself, or report what \
             you have with complete_task"
        );
    }
    if parent.is_linked() {
        bail!("{} runs on another machine", parent.name);
    }
    if req.brief.trim().is_empty() {
        bail!("'task' is required: the brief the worker is given");
    }
    if req.brief.len() > bus::MAX_MESSAGE_BYTES {
        bail!("'task' exceeds {} bytes", bus::MAX_MESSAGE_BYTES);
    }
    if app.db.queued_workers(&parent.project_id)?.len() >= MAX_QUEUED_PER_PROJECT {
        bail!(
            "this project already has {MAX_QUEUED_PER_PROJECT} spawns queued — wait for \
             some to finish, or cancel_worker the ones you no longer need"
        );
    }
    let machine = machine_choice(app, &parent.project_id, req.machine)?;
    if let Some(runtime) = req.runtime.filter(|_| machine.as_deref() == Some(HERE)) {
        botmgmt::check_runtime_available(app, runtime)?;
    }
    // Refused now rather than queued: waiting will not make the chain shorter.
    delegation_chain(app, parent)?;
    let name = match req.name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(raw) => botmgmt::validate_name(app, &parent.project_id, raw, None)?,
        None => next_name(app, &parent.project_id)?,
    };
    let deadline_hours = req
        .deadline_hours
        .unwrap_or(bus::DEFAULT_TASK_DEADLINE_HOURS)
        .clamp(1, MAX_DEADLINE_HOURS);
    let description = req
        .description
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Worker for {}: {}", parent.name, headline(req.brief)));
    let worker = app.db.insert_worker(&NewWorker {
        project_id: &parent.project_id,
        parent_bot_id: &parent.id,
        name: &name,
        brief: req.brief,
        description: &description,
        instructions: req.instructions.map(str::trim).unwrap_or_default(),
        runtime: req.runtime,
        machine: machine.as_deref(),
        deadline_hours,
    })?;
    place_queued(app, &parent.project_id).await;
    Ok(app.db.get_worker(&worker.id)?.unwrap_or(worker))
}

/// Drop a queued spawn, or cancel a running worker's task, which retires it.
pub fn cancel(
    app: &Arc<AppState>,
    parent: &Bot,
    name: &str,
    reason: &str,
) -> anyhow::Result<Worker> {
    let worker = app
        .db
        .active_worker_by_name(&parent.id, name)?
        .ok_or_else(|| anyhow::anyhow!("you have no queued or running worker named '{name}'"))?;
    match worker.state {
        WorkerState::Queued => {
            app.db
                .finish_worker(&worker.id, WorkerState::Cancelled, Some(reason))?;
        }
        _ => {
            if let Some(task) = worker
                .task_id
                .as_deref()
                .map(|id| app.db.get_task(id))
                .transpose()?
                .flatten()
            {
                let body = format!(
                    "Task {} was cancelled. Stop work on it; no one waits on the result.{}",
                    task.id,
                    if reason.is_empty() {
                        String::new()
                    } else {
                        format!(" Reason: {reason}")
                    }
                );
                close_cancelled(app, &bot_sender(parent), &task, &body)?;
            }
            app.db
                .finish_worker(&worker.id, WorkerState::Cancelled, Some(reason))?;
        }
    }
    app.workers.nudge();
    Ok(app.db.get_worker(&worker.id)?.unwrap_or(worker))
}

/// Normalise a requested machine: `None` for any, [`HERE`], or a linked
/// peer's name.
fn machine_choice(
    app: &AppState,
    project_id: &str,
    requested: Option<&str>,
) -> anyhow::Result<Option<String>> {
    let Some(raw) = requested.map(str::trim).filter(|m| !m.is_empty()) else {
        return Ok(None);
    };
    if raw.eq_ignore_ascii_case(HERE) || raw.eq_ignore_ascii_case("local") {
        return Ok(Some(HERE.to_string()));
    }
    if raw.eq_ignore_ascii_case("any") {
        return Ok(None);
    }
    Ok(Some(linked_peer_named(app, project_id, raw)?.name))
}

/// The first free `worker-N` in the project.
fn next_name(app: &Arc<AppState>, project_id: &str) -> anyhow::Result<String> {
    (1..=1000)
        .map(|n| format!("worker-{n}"))
        .find(|name| botmgmt::validate_name(app, project_id, name, None).is_ok())
        .ok_or_else(|| anyhow::anyhow!("no free worker name; pass 'name'"))
}

/// The brief's first line, short enough for a description.
fn headline(brief: &str) -> String {
    let line = brief
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    match line.char_indices().nth(120) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headline_is_the_first_line_cut_short() {
        assert_eq!(headline("\n  Write chapter 3\nmore"), "Write chapter 3");
        let long = "x".repeat(200);
        assert_eq!(headline(&long).chars().count(), 121);
    }
}
