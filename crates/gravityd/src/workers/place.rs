//! Placing queued spawns: finding a machine with a free slot, creating the
//! worker there, and delegating its brief to it.

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
///
/// Nothing here waits on git or on a slow peer while holding up another
/// spawn: a worker here is created and handed its task in one quick step,
/// under a lock so two passes never give out one slot, and what is left is
/// offered to linked machines in parallel, outside it.
pub async fn place_queued(app: &Arc<AppState>, project_id: &str) {
    let offers = {
        let _placing = app.workers.placing.lock().await;
        match place_here(app, project_id) {
            Ok(offers) => offers,
            Err(error) => {
                tracing::warn!(project_id, %error, "placing workers failed");
                return;
            }
        }
    };
    futures::future::join_all(offers.into_iter().map(|offer| offer_to_peers(app, offer))).await;
}

/// A machine a spawn may go to.
enum Place {
    Here,
    Peer(Peer),
}

/// A spawn that did not fit here, and the linked machines to try instead.
struct Offer {
    worker: Worker,
    parent: Bot,
    peers: Vec<Peer>,
}

fn place_here(app: &Arc<AppState>, project_id: &str) -> anyhow::Result<Vec<Offer>> {
    let mut here_full = false;
    let mut offers = Vec::new();
    for worker in app.db.queued_workers(project_id)? {
        // Already being offered to a peer by another pass.
        if app.workers.is_offering(&worker.id) {
            continue;
        }
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
        let mut peers = Vec::new();
        let mut placed = false;
        for place in places {
            match place {
                Place::Here if here_full => {}
                Place::Here => match start_here(app, &worker, &parent) {
                    Ok(()) => {
                        placed = true;
                        break;
                    }
                    Err(error) if is_full(&error) => here_full = true,
                    Err(error) => {
                        fail(app, &worker, &parent, &error)?;
                        placed = true;
                        break;
                    }
                },
                Place::Peer(peer) => peers.push(peer),
            }
        }
        if placed {
            continue;
        }
        if peers.is_empty() {
            let reason = match worker.machine.as_deref() {
                Some(machine) if machine != HERE => format!("waiting for {machine} to come online"),
                _ => "waiting for a free worker slot".to_string(),
            };
            wait(app, &worker, &reason)?;
        } else {
            app.workers.start_offering(&worker.id);
            offers.push(Offer {
                worker,
                parent,
                peers,
            });
        }
    }
    Ok(offers)
}

/// Try a spawn on each linked machine in turn, until one takes it.
async fn offer_to_peers(app: &Arc<AppState>, offer: Offer) {
    let Offer {
        worker,
        parent,
        peers,
    } = offer;
    let mut busy = Vec::new();
    let mut outcome = Ok(false);
    for peer in &peers {
        match start_there(app, &worker, &parent, peer).await {
            Ok(()) => {
                outcome = Ok(true);
                break;
            }
            Err(error) if is_full(&error) => busy.push(peer.name.clone()),
            Err(error) => {
                outcome = fail(app, &worker, &parent, &error).map(|()| true);
                break;
            }
        }
    }
    let result = match outcome {
        Ok(true) => Ok(()),
        Ok(false) => wait(
            app,
            &worker,
            &format!(
                "waiting for a free worker slot ({} busy too)",
                busy.join(", ")
            ),
        ),
        Err(error) => Err(error),
    };
    app.workers.stop_offering(&worker.id);
    if let Err(error) = result {
        tracing::warn!(worker = %worker.name, %error, "offering a worker failed");
    }
}

/// Note why a spawn still waits, telling clients when that changed.
fn wait(app: &AppState, worker: &Worker, reason: &str) -> anyhow::Result<()> {
    if app.db.note_worker_waiting(&worker.id, Some(reason))? {
        super::changed(app, &worker.project_id);
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

fn start_here(app: &Arc<AppState>, worker: &Worker, parent: &Bot) -> anyhow::Result<()> {
    let chain = delegation_chain(app, parent)?;
    // A worker created for this spawn that never got its task — the daemon
    // stopped between the two — gets it now rather than blocking the name.
    if let Some(bot) = untasked_worker(app, worker, parent)? {
        return hand_over(app, worker, parent, &bot, chain);
    }
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
    hand_over(app, worker, parent, &created.bot, chain)
}

/// The live worker bot here named for this spawn, made by its parent, that
/// was never given a task.
fn untasked_worker(app: &AppState, worker: &Worker, parent: &Bot) -> anyhow::Result<Option<Bot>> {
    let Some(bot) = app.db.get_bot_by_name(&worker.project_id, &worker.name)? else {
        return Ok(None);
    };
    let ours = bot.temporary
        && !bot.is_linked()
        && bot.created_by_bot_id.as_deref() == Some(parent.id.as_str());
    if !ours || app.db.latest_task_to(&bot.id)?.is_some() {
        return Ok(None);
    }
    Ok(Some(bot))
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
    // A linked project with no repository of its own adopts this one, so the
    // worker there is told where to clone from and push to.
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
    let started = app.db.start_worker(&worker.id, &bot.id, &task.id)?;
    super::changed(app, &worker.project_id);
    if !started {
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
    super::changed(app, &worker.project_id);
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
