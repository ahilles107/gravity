//! Bots created, edited and deleted on the peer, in a linked project. The bot
//! is real there and a stand-in here; the stand-in is updated from the reply
//! at once rather than waiting for the roster that follows.
//!
//! A bot asking (over MCP) is named by `as_bot_id`, its id on the asking
//! daemon. The peer resolves that to the bot's stand-in and holds it to the
//! same rule as a local bot: it manages only bots it created.

use std::sync::Arc;

use bus::{Bot, BotRuntime, Peer, RemoteBot};
use serde_json::{json, Value};

use crate::app::AppState;
use crate::botmgmt::{self, IdentityEdit};
use crate::db::Actor;

use super::{refuse, roster};

/// The identity fields a request carries: `name`, `description`,
/// `instructions`, `avatar` and `runtime`, any of them absent.
pub fn fields(source: &Value, with_name: bool) -> Value {
    let mut out = json!({});
    for key in ["name", "description", "instructions", "avatar", "runtime"] {
        if key == "name" && !with_name {
            continue;
        }
        if let Some(value) = source.get(key).filter(|v| v.is_string()) {
            out[key] = value.clone();
        }
    }
    out
}

/// The link a project here has through a peer that is up, or why not.
fn linked_peer(app: &AppState, project_id: &str, peer_id: &str) -> anyhow::Result<Peer> {
    let peer = app
        .db
        .get_peer(peer_id)?
        .filter(|p| p.revoked_at.is_none())
        .ok_or_else(|| refuse("not_found", "peer not found or revoked"))?;
    if app.db.project_link(project_id, peer_id)?.is_none() {
        return Err(refuse(
            "not_linked",
            format!("this project is not linked with {}", peer.name),
        ));
    }
    if !app.peers.is_online(peer_id) {
        return Err(refuse("unavailable", format!("{} is offline", peer.name)));
    }
    Ok(peer)
}

// ---- this side asks ----

/// Creates a bot on the peer in the project linked with `project_id`, and
/// returns its stand-in here. `creator` is the bot asking, when one is.
pub async fn create(
    app: &Arc<AppState>,
    project_id: &str,
    peer_id: &str,
    identity: Value,
    creator: Option<&Bot>,
) -> anyhow::Result<Bot> {
    let peer = linked_peer(app, project_id, peer_id)?;
    let mut frame = identity;
    frame["type"] = json!("create_bot");
    frame["project_id"] = json!(project_id);
    if let Some(creator) = creator {
        frame["as_bot_id"] = json!(creator.id);
    }
    let result = app.peers.request(peer_id, frame).await?;
    let remote: RemoteBot = serde_json::from_value(result["bot"].clone())?;
    let bot = roster::stand_in(app, &peer, project_id, &remote)?;
    if let Some(creator) = creator {
        app.db.set_bot_creator(&bot.id, &creator.id)?;
    }
    tracing::info!(peer = %peer.name, bot = %bot.name, "bot created on peer");
    Ok(app.db.get_bot(&bot.id)?.unwrap_or(bot))
}

/// Edits the bot a stand-in stands for, and returns the updated stand-in.
pub async fn update(
    app: &Arc<AppState>,
    stand_in: &Bot,
    identity: Value,
    asking: Option<&Bot>,
) -> anyhow::Result<Bot> {
    let (peer, remote_id) = target(app, stand_in)?;
    let mut frame = identity;
    frame["type"] = json!("update_bot");
    frame["bot_id"] = json!(remote_id);
    if let Some(asking) = asking {
        frame["as_bot_id"] = json!(asking.id);
    }
    let result = app.peers.request(&peer.id, frame).await?;
    let remote: RemoteBot = serde_json::from_value(result["bot"].clone())?;
    roster::stand_in(app, &peer, &stand_in.project_id, &remote)
}

/// Deletes the bot a stand-in stands for, and archives the stand-in.
pub async fn delete(
    app: &Arc<AppState>,
    stand_in: &Bot,
    reason: Option<&str>,
    asking: Option<&Bot>,
) -> anyhow::Result<()> {
    let (peer, remote_id) = target(app, stand_in)?;
    let mut frame = json!({ "type": "delete_bot", "bot_id": remote_id, "reason": reason });
    if let Some(asking) = asking {
        frame["as_bot_id"] = json!(asking.id);
    }
    app.peers.request(&peer.id, frame).await?;
    roster::archive_stand_in(
        app,
        &stand_in.id,
        reason.unwrap_or("deleted on its machine"),
    )
}

/// Whether edits to this bot belong on its peer: it stands in for a bot there
/// and its project is linked through that peer.
pub fn is_mirrored(app: &AppState, bot: &Bot) -> bool {
    bot.peer_id
        .as_deref()
        .is_some_and(|peer| matches!(app.db.project_link(&bot.project_id, peer), Ok(Some(_))))
}

fn target(app: &AppState, stand_in: &Bot) -> anyhow::Result<(Peer, String)> {
    let (Some(peer_id), Some(remote_id)) = (&stand_in.peer_id, &stand_in.remote_bot_id) else {
        anyhow::bail!("{} runs on this machine", stand_in.name);
    };
    let peer = linked_peer(app, &stand_in.project_id, peer_id)?;
    Ok((peer, remote_id.clone()))
}

// ---- the peer asks ----

/// The bot the peer means to act as, held to its own project.
fn acting_bot(
    app: &AppState,
    peer: &Peer,
    project_id: &str,
    frame: &Value,
) -> anyhow::Result<Option<Bot>> {
    let Some(remote_id) = frame.get("as_bot_id").and_then(Value::as_str) else {
        return Ok(None);
    };
    let stand_in = app
        .db
        .linked_bot(&peer.id, remote_id, project_id)?
        .ok_or_else(|| anyhow::anyhow!("the asking bot is not in this linked project"))?;
    Ok(Some(stand_in))
}

fn actor(bot: Option<&Bot>) -> Actor<'_> {
    match bot {
        Some(bot) => Actor::Bot {
            id: &bot.id,
            project_id: &bot.project_id,
        },
        None => Actor::User,
    }
}

fn edit(frame: &Value, with_name: bool) -> IdentityEdit<'_> {
    IdentityEdit {
        name: with_name
            .then(|| frame.get("name").and_then(Value::as_str))
            .flatten(),
        description: frame.get("description").and_then(Value::as_str),
        instructions: frame.get("instructions").and_then(Value::as_str),
        avatar: frame.get("avatar").and_then(Value::as_str),
    }
}

/// A bot here the peer may manage: in a project linked through it, and, when
/// a bot is asking, one that bot created.
fn managed_bot(app: &AppState, peer: &Peer, frame: &Value) -> anyhow::Result<(Bot, Option<Bot>)> {
    let bot_id = frame
        .get("bot_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("'bot_id' is required"))?;
    let bot = app
        .db
        .get_live_bot(bot_id)?
        .filter(|b| !b.is_linked())
        .ok_or_else(|| refuse("not_found", "no such bot runs here"))?;
    if app.db.project_link(&bot.project_id, &peer.id)?.is_none() {
        return Err(refuse(
            "not_linked",
            format!("{}'s project is not linked", bot.name),
        ));
    }
    let asking = acting_bot(app, peer, &bot.project_id, frame)?;
    if let Some(asking) = &asking {
        anyhow::ensure!(
            bot.created_by_bot_id.as_deref() == Some(asking.id.as_str()),
            "'{}' was not created by you; you can only manage bots you created",
            bot.name
        );
    }
    Ok((bot, asking))
}

/// Keeps a missing runtime recognisable on the other side of the link.
fn coded(error: anyhow::Error) -> anyhow::Error {
    if error
        .downcast_ref::<botmgmt::RuntimeUnavailable>()
        .is_some()
    {
        refuse("runtime_unavailable", format!("{error:#}"))
    } else {
        error
    }
}

pub(super) fn serve_create(
    app: &Arc<AppState>,
    peer: &Peer,
    frame: &Value,
) -> anyhow::Result<Value> {
    let remote_project = frame
        .get("project_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("'project_id' is required"))?;
    let link = app
        .db
        .project_link_by_remote(&peer.id, remote_project)?
        .ok_or_else(|| refuse("not_linked", "that project is not linked with one here"))?;
    let creator = acting_bot(app, peer, &link.project_id, frame)?;
    let runtime = botmgmt::requested_runtime(frame)?.unwrap_or(app.cfg.default_bot_runtime);
    botmgmt::check_runtime_available(app, runtime).map_err(coded)?;
    // A worker counts against this machine's worker cap; when that is full,
    // the asker keeps its spawn queued and tries elsewhere.
    let temporary = frame.get("temporary").and_then(Value::as_bool) == Some(true);
    let repo = shared_repo(app, &link.project_id, frame)?;
    let create = if temporary {
        botmgmt::create_worker_bot
    } else {
        botmgmt::create_bot_with_runtime
    };
    let created = create(
        app,
        &link.project_id,
        &edit(frame, true),
        creator.as_ref(),
        &actor(creator.as_ref()),
        runtime,
    )
    .map_err(|error| match error.downcast_ref::<botmgmt::WorkersFull>() {
        Some(full) => refuse("at_capacity", full.to_string()),
        None => error,
    })?;
    // Handled off the link's read loop, so a clone may take its time here.
    if let Some(repo) = repo {
        crate::workers::check_out_or_retire(app, &created.bot, &repo)
            .map_err(|error| refuse("repo_failed", format!("{error:#}")))?;
    }
    // Before the reply, not with the roster that follows it: the asker may
    // message its new bot the moment it hears back.
    app.db.expose_bot_to_peer(&peer.id, &created.bot.id)?;
    tracing::info!(peer = %peer.name, bot = %created.bot.name, "peer created a bot in a linked project");
    Ok(json!({ "bot": super::inbound::remote_view(&created.bot, String::new()) }))
}

/// The repository a worker is to check out, from the asker. A linked project
/// is one team, so the asker's repository becomes this side's too when it
/// has none of its own.
fn shared_repo(
    app: &AppState,
    project_id: &str,
    frame: &Value,
) -> anyhow::Result<Option<bus::ProjectRepo>> {
    let Some(raw) = frame.get("repo").filter(|r| r.is_object()) else {
        return Ok(None);
    };
    let url = raw.get("url").and_then(Value::as_str).unwrap_or_default();
    let branch = raw.get("branch").and_then(Value::as_str);
    let repo = bus::ProjectRepo::parse(url, branch).map_err(anyhow::Error::msg)?;
    if app.db.project_repo(project_id)?.is_none() {
        app.db.set_project_repo(project_id, Some(&repo))?;
    }
    Ok(Some(repo))
}

pub(super) fn serve_update(
    app: &Arc<AppState>,
    peer: &Peer,
    frame: &Value,
) -> anyhow::Result<Value> {
    let (bot, asking) = managed_bot(app, peer, frame)?;
    let updated = botmgmt::apply_identity_edit(
        app,
        &bot,
        &edit(frame, asking.is_none()),
        &actor(asking.as_ref()),
    )?;
    let runtime: Option<BotRuntime> = botmgmt::requested_runtime(frame)?;
    let updated = match runtime.filter(|r| *r != updated.runtime) {
        Some(runtime) => botmgmt::set_bot_runtime(app, &updated, runtime).map_err(coded)?,
        None => updated,
    };
    Ok(json!({ "bot": super::inbound::remote_view(&updated, String::new()) }))
}

pub(super) fn serve_delete(
    app: &Arc<AppState>,
    peer: &Peer,
    frame: &Value,
) -> anyhow::Result<Value> {
    let (bot, asking) = managed_bot(app, peer, frame)?;
    let reason = frame.get("reason").and_then(Value::as_str);
    botmgmt::archive_bot(app, &bot, &actor(asking.as_ref()), reason)?;
    Ok(json!({}))
}
