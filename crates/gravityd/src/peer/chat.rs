//! A linked bot's real chat, read across the peer link. The daemon that runs
//! the bot serves its turns, step details, images and files to the peer it is
//! linked to, and streams turn updates while the peer has the chat open; the
//! other daemon asks on behalf of its app and swaps in its own bot id.

use std::sync::Arc;

use bus::{Bot, Peer};
use serde_json::{json, Value};
use tokio::sync::broadcast::error::RecvError;

use crate::app::AppState;
use crate::chat::files::{self, Scope};
use crate::events::Push;

/// A peer asks about one of this daemon's bots that is linked to it.
pub(super) fn serve(app: &AppState, peer: &Peer, frame: &Value) -> anyhow::Result<Value> {
    let bot_id = frame["bot_id"].as_str().unwrap_or_default();
    let bot = app
        .db
        .get_live_bot(bot_id)?
        .filter(|b| !b.is_linked())
        .ok_or_else(|| anyhow::anyhow!("no bot with id {bot_id} runs here"))?;
    anyhow::ensure!(
        app.db.is_exposed_to_peer(&peer.id, &bot.id)?,
        "{} is not linked to this daemon",
        bot.name
    );
    let text = |key: &str| frame[key].as_str().unwrap_or_default();
    match frame["type"].as_str().unwrap_or_default() {
        "chat" => {
            app.peers.watch_chat(&peer.id, &bot.id);
            let limit = frame["limit"].as_u64().unwrap_or(30).clamp(1, 200) as usize;
            let (turns, has_more) = app.chat.page(app, &bot, frame["before"].as_str(), limit)?;
            Ok(json!({ "turns": turns, "has_more": has_more }))
        }
        "chat_step" => Ok(json!({ "detail": app.chat.step(app, &bot, text("item_id"))? })),
        "bot_commands" => {
            let limit = frame["limit"].as_u64().unwrap_or(100).clamp(1, 500) as usize;
            Ok(json!({ "commands": app.chat.commands(app, &bot, limit)? }))
        }
        "browser_activity" => {
            let limit = frame["limit"].as_u64().unwrap_or(100).clamp(1, 500) as usize;
            Ok(json!({ "activity": app.chat.browser_activity(app, &bot, limit)? }))
        }
        "chat_image" => {
            let (mime, base64) = app.chat.image(app, &bot, text("image_id"))?;
            Ok(json!({ "mime": mime, "base64": base64 }))
        }
        "read_file" => {
            let project = app
                .db
                .get_project(&bot.project_id)?
                .ok_or_else(|| anyhow::anyhow!("project missing"))?;
            let file = files::read_file(app, Scope::Bot(&bot, &project), text("path"))?;
            Ok(json!({ "file": file }))
        }
        other => anyhow::bail!("unknown chat request '{other}'"),
    }
}

/// Asks the peer a linked bot runs on, in the remote bot's own id.
pub async fn ask(app: &Arc<AppState>, linked: &Bot, mut frame: Value) -> anyhow::Result<Value> {
    let (Some(peer_id), Some(remote)) = (&linked.peer_id, &linked.remote_bot_id) else {
        anyhow::bail!("{} is not a linked bot", linked.name);
    };
    frame["bot_id"] = json!(remote);
    app.peers
        .request(peer_id, frame)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Points turns read from a peer at the local linked bot.
pub fn localize(mut turns: Value, bot_id: &str) -> Value {
    for turn in turns.as_array_mut().into_iter().flatten() {
        turn["bot_id"] = json!(bot_id);
    }
    turns
}

/// Streams this daemon's turn updates to the peers watching each chat.
pub async fn forward(app: Arc<AppState>) {
    let mut pushes = app.events.subscribe_push();
    loop {
        match pushes.recv().await {
            Ok(Push::ChatTurns { bot_id, turns }) => {
                for peer in app.peers.chat_watchers(&bot_id) {
                    app.peers.notify(
                        &peer,
                        json!({ "type": "chat_turns", "bot_id": bot_id, "turns": turns }),
                    );
                }
            }
            Ok(_) => {}
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}

/// Turn updates from a peer, for the linked bots standing in for its bot.
pub(super) fn receive_turns(app: &AppState, peer_id: &str, frame: &Value) {
    let remote = frame["bot_id"].as_str().unwrap_or_default();
    let Ok(linked) = app.db.linked_bots_for(peer_id, remote) else {
        return;
    };
    for bot in linked {
        let turns = localize(frame["turns"].clone(), &bot.id);
        if let Ok(turns) = serde_json::from_value(turns) {
            app.events.push(Push::ChatTurns {
                bot_id: bot.id,
                turns,
            });
        }
    }
}
