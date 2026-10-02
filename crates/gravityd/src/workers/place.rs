//! Placing queued spawns: finding a machine with a free slot, creating the
//! worker there, and delegating its brief to it.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::bail;
use bus::{Bot, MessageKind, Peer, Worker, WorkerState, MAX_TASK_HOPS};
use serde_json::json;

use crate::app::AppState;
use crate::botmgmt::{self, IdentityEdit, WorkersFull};
use crate::db::Actor;
use crate::mcp::bot_sender;
use crate::mcp::tasks::close_cancelled;
use crate::messaging::{self, daemon_sender, Dm};
use crate::peer::remote_bots;

use super::{release_orphan, HERE};

/// Instructions for a worker spawned without any. Non-empty so the charter
/// for a bot "created from a name alone" — which tells it to ask what it is
/// for — never applies: a worker's task is its whole charter.
const DEFAULT_INSTRUCTIONS: &str =
    "Do the one task you are given and report it with complete_task.";

/// Place what a project has queued, oldest first, while slots last.
pub async fn place_queued(app: &Arc<AppState>, project_id: &str) {
    let _placing = app.workers.placing.lock().await;
    if let Err(error) = place_locked(app, project_id).await {
        tracing::warn!(project_id, %error, "placing workers failed");
    }
}

/// A machine a spawn may go to.
enum Place {
    Here,
    Peer(Peer),
}

async fn place_locked(app: &Arc<AppState>, project_id: &str) -> anyhow::Result<()> {
    // Machines found full this pass are not asked again until the next one.
    let mut here_full = false;
    let mut full: HashSet<String> = HashSet::new();
    for worker in app.db.queued_workers(project_id)? {
        let Some(parent) = app.db.get_live_bot(&worker.parent_bot_id)? else {
            release_orphan(app, &worker)?;
            continue;
        };
        let places = match places_for(app, &worker) {
            Ok(places) => places,
            Err(error) => {
                fail(app, &worker, &parent, &error)?;
                continue;
            }
        };
        let mut waiting = None;
        for place in places {
            let attempt = match &place {
                Place::Here if here_full => continue,
                Place::Peer(peer) if full.contains(&peer.id) => continue,
                Place::Here => start_here(app, &worker, &parent).await,
                Place::Peer(peer) => start_there(app, &worker, &parent, peer).await,
            };
            match attempt {
                Ok(()) => {
                    waiting = None;
                    break;
                }
                Err(error) if is_full(&error) => {
                    match &place {
                        Place::Here => here_full = true,
                        Place::Peer(peer) => {
                            full.insert(peer.id.clone());
                        }
                    }
                    waiting = Some(match &place {
                        Place::Here => "waiting for a free worker slot".to_string(),
                        Place::Peer(peer) => {
                            format!("waiting for a free worker slot ({} is busy)", peer.name)
                        }
                    });
                }
                Err(error) => {
                    fail(app, &worker, &parent, &error)?;
                    waiting = None;
                    break;
                }
            }
        }
        if let Some(reason) = waiting {
            app.db.note_worker_waiting(&worker.id, Some(&reason))?;
        }
    }
    Ok(())
}

/// Whether an attempt failed only for want of a slot, or of a reachable
/// machine — both of which waiting can fix.
fn is_full(error: &anyhow::Error) -> bool {
    error.downcast_ref::<WorkersFull>().is_some()
        || matches!(
            crate::peer::error_code(error, ""),
            "at_capacity" | "unavailable"
        )
}

/// Where a spawn may go, in the order to try: here first, then each machine
/// the project is linked with that is online.
fn places_for(app: &AppState, worker: &Worker) -> anyhow::Result<Vec<Place>> {
    match worker.machine.as_deref() {
        Some(HERE) => Ok(vec![Place::Here]),
        Some(name) => {
            let peer = linked_peer_named(app, &worker.project_id, name)?;
            Ok(if app.peers.is_online(&peer.id) {
                vec![Place::Peer(peer)]
            } else {
                Vec::new()
            })
        }
        None => {
            let mut places = vec![Place::Here];
            for link in app.db.project_links(&worker.project_id)? {
                if let Some(peer) = app.db.get_peer(&link.peer_id)? {
                    if peer.revoked_at.is_none() && app.peers.is_online(&peer.id) {
                        places.push(Place::Peer(peer));
                    }
                }
            }
            Ok(places)
        }
    }
}

pub(super) fn linked_peer_named(
    app: &AppState,
    project_id: &str,
    name: &str,
) -> anyhow::Result<Peer> {
    let peer = app
        .db
        .get_peer_by_name(name)?
        .filter(|p| p.revoked_at.is_none())
        .ok_or_else(|| anyhow::anyhow!("no machine named '{name}'"))?;
    if app.db.project_link(project_id, &peer.id)?.is_none() {
        bail!("this project is not linked with {name}, so no worker can run there");
    }
    Ok(peer)
}

/// The chain a worker's task extends: the parent's newest open task, so the
/// hop limit holds through workers as through any delegation.
pub(super) fn delegation_chain(app: &AppState, parent: &Bot) -> anyhow::Result<(i64, String)> {
    let (hop, chain) = app
        .db
        .newest_open_task_for(&parent.id)?
        .map(|t| (t.hop_count, t.origin_chain))
        .unwrap_or((0, String::new()));
    if hop + 1 > MAX_TASK_HOPS {
        bail!(
            "this delegation chain is {hop} hops deep (limit {MAX_TASK_HOPS}) — do the \
             work yourself, or report what you have with complete_task"
        );
    }
    let chain = if chain.is_empty() {
        parent.id.clone()
    } else {
        format!("{chain},{}", parent.id)
    };
    Ok((hop + 1, chain))
}

fn instructions(worker: &Worker) -> &str {
    if worker.instructions.trim().is_empty() {
        DEFAULT_INSTRUCTIONS
    } else {
        &worker.instructions
    }
}

async fn start_here(app: &Arc<AppState>, worker: &Worker, parent: &Bot) -> anyhow::Result<()> {
    let chain = delegation_chain(app, parent)?;
    let runtime = worker.runtime.unwrap_or(parent.runtime);
    botmgmt::check_runtime_available(app, runtime)?;
    let edit = IdentityEdit {
        name: Some(&worker.name),
        description: Some(&worker.description),
        instructions: Some(instructions(worker)),
        avatar: None,
    };
    let actor = Actor::Bot {
        id: &parent.id,
        project_id: &parent.project_id,
    };
    let created = botmgmt::create_worker_bot(
        app,
        &worker.project_id,
        &edit,
        Some(parent),
        &actor,
        runtime,
    )?;
    if let Some(repo) = app.db.project_repo(&worker.project_id)? {
        let (app, bot) = (app.clone(), created.bot.clone());
        tokio::task::spawn_blocking(move || super::check_out_or_retire(&app, &bot, &repo))
            .await??;
    }
    hand_over(app, worker, parent, &created.bot, chain)
}

async fn start_there(
    app: &Arc<AppState>,
    worker: &Worker,
    parent: &Bot,
    peer: &Peer,
) -> anyhow::Result<()> {
    let chain = delegation_chain(app, parent)?;
    let mut identity = json!({
        "name": worker.name,
        "description": worker.description,
        "instructions": instructions(worker),
        "temporary": true,
    });
    if let Some(runtime) = worker.runtime {
        identity["runtime"] = json!(runtime.as_str());
    }
    // The worker's machine checks the repository out itself, with its own
    // credentials, before it answers.
    if let Some(repo) = app.db.project_repo(&worker.project_id)? {
        identity["repo"] = json!(repo);
    }
    let stand_in =
        remote_bots::create(app, &worker.project_id, &peer.id, identity, Some(parent)).await?;
    hand_over(app, worker, parent, &stand_in, chain)
}

/// Delegate the brief to the worker that now exists for it.
fn hand_over(
    app: &Arc<AppState>,
    worker: &Worker,
    parent: &Bot,
    bot: &Bot,
    (hop, chain): (i64, String),
) -> anyhow::Result<()> {
    let deadline_at = chrono::Utc::now() + chrono::Duration::hours(worker.deadline_hours);
    let sender = bot_sender(parent);
    let msg = messaging::send_dm(
        &app.db,
        &app.events,
        Dm::new(&bot.id, &sender, MessageKind::Task, &worker.brief),
    )?;
    let task = app.db.create_task(
        &msg.id,
        Some(&parent.id),
        &bot.id,
        Some(deadline_at),
        hop,
        &chain,
    )?;
    if !app.db.start_worker(&worker.id, &bot.id, &task.id)? {
        // Cancelled while it was being placed; the worker retires with it.
        let body = format!("Task {} was cancelled before you began. Stop.", task.id);
        close_cancelled(app, &daemon_sender(), &task, &body)?;
    }
    tracing::info!(worker = %worker.name, bot_id = %bot.id, "worker started");
    Ok(())
}

/// Give up on a spawn that waiting will not fix, and tell its parent.
fn fail(
    app: &Arc<AppState>,
    worker: &Worker,
    parent: &Bot,
    error: &anyhow::Error,
) -> anyhow::Result<()> {
    let reason = format!("{error:#}");
    if !app
        .db
        .finish_worker(&worker.id, WorkerState::Failed, Some(&reason))?
    {
        return Ok(());
    }
    tracing::warn!(worker = %worker.name, %reason, "worker could not start");
    let body = format!(
        "Worker {} could not start, and has been dropped from the queue: {reason}",
        worker.name
    );
    messaging::send_dm(
        &app.db,
        &app.events,
        Dm::new(&parent.id, &daemon_sender(), MessageKind::Note, &body),
    )?;
    Ok(())
}
