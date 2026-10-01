//! Requests a peer makes of this daemon. A peer can see which bots exist,
//! link one, and deliver messages to the bots linked to it. Nothing else.

use std::sync::Arc;

use bus::{Peer, RemoteBot};
use serde_json::{json, Value};

use crate::app::AppState;

pub(super) fn handle(app: &Arc<AppState>, peer_id: &str, frame: &Value) -> anyhow::Result<Value> {
    let peer = app
        .db
        .get_peer(peer_id)?
        .filter(|p| p.revoked_at.is_none())
        .ok_or_else(|| anyhow::anyhow!("peer revoked"))?;
    match frame.get("type").and_then(Value::as_str).unwrap_or("") {
        "ping" => Ok(json!({})),
        "list_bots" => list_bots(app),
        "link" => link(app, &peer, frame),
        "chat" | "chat_step" | "chat_image" | "read_file" => super::chat::serve(app, &peer, frame),
        "message" => {
            let message = serde_json::from_value(frame["message"].clone())?;
            let received = super::receive::receive(app, &peer, message)?;
            Ok(serde_json::to_value(received)?)
        }
        other => anyhow::bail!("unknown peer request '{other}'"),
    }
}

/// News a peer sends without asking anything back.
pub(super) fn event(app: &Arc<AppState>, peer_id: &str, frame: &Value) {
    if frame["type"] == "chat_turns" {
        super::chat::receive_turns(app, peer_id, frame);
    }
}

/// The bots a peer could link: every live bot that runs here.
fn list_bots(app: &Arc<AppState>) -> anyhow::Result<Value> {
    let projects: std::collections::HashMap<String, String> = app
        .db
        .list_projects()?
        .into_iter()
        .map(|p| (p.id.clone(), crate::db::Db::display_project_name(&p)))
        .collect();
    let bots: Vec<RemoteBot> = app
        .db
        .list_bots(None)?
        .into_iter()
        .filter(|bot| !bot.is_linked())
        .map(|bot| {
            let project = projects.get(&bot.project_id).cloned().unwrap_or_default();
            remote_view(&bot, project)
        })
        .collect();
    Ok(json!({ "bots": bots }))
}

/// Exposes one local bot to the peer, which is about to stand it in.
fn link(app: &Arc<AppState>, peer: &Peer, frame: &Value) -> anyhow::Result<Value> {
    let bot_id = frame
        .get("bot_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("'bot_id' is required"))?;
    let bot = app
        .db
        .get_live_bot(bot_id)?
        .filter(|b| !b.is_linked())
        .ok_or_else(|| anyhow::anyhow!("no bot with id {bot_id} runs on this daemon"))?;
    app.db.expose_bot_to_peer(&peer.id, &bot.id)?;
    tracing::info!(peer = %peer.name, bot = %bot.name, "bot linked to peer");
    Ok(json!({ "bot": remote_view(&bot, String::new()) }))
}

pub(super) fn remote_view(bot: &bus::Bot, project: String) -> RemoteBot {
    RemoteBot {
        id: bot.id.clone(),
        name: bot.name.clone(),
        description: bot.description.clone(),
        avatar: bot.avatar.clone(),
        runtime: bot.runtime,
        project,
    }
}
