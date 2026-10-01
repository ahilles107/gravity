//! Forwarding a delivery whose recipient is a linked bot: the delivery
//! worker's branch for bots that run on a peer.

use std::sync::Arc;

use bus::{Bot, Message, MessageKind, Peer, SenderKind, TaskState};
use serde_json::json;

use crate::app::AppState;

use super::frames::{ClosesFrame, FromFrame, MessageFrame, Received, TaskFrame};
use super::hub::PeerError;

pub enum ForwardError {
    /// The peer is not connected. Nothing was sent; try again later.
    Offline,
    Failed(anyhow::Error),
}

/// Hands `msg` to the peer `target` runs on. `Ok` once the peer has stored it,
/// or once it is clear the message is not the peer's to see.
pub async fn forward(app: &Arc<AppState>, target: &Bot, msg: &Message) -> Result<(), ForwardError> {
    let fail = |e: anyhow::Error| ForwardError::Failed(e);
    let peer_id = target
        .peer_id
        .clone()
        .ok_or_else(|| fail(anyhow::anyhow!("not a linked bot")))?;
    let peer = app
        .db
        .get_peer(&peer_id)
        .map_err(fail)?
        .filter(|p| p.revoked_at.is_none())
        .ok_or_else(|| fail(anyhow::anyhow!("peer revoked")))?;
    if !app.peers.is_online(&peer.id) {
        return Err(ForwardError::Offline);
    }
    let frame = {
        let app = app.clone();
        let (peer, target, msg) = (peer.clone(), target.clone(), msg.clone());
        tokio::task::spawn_blocking(move || build(&app, &peer, &target, &msg))
            .await
            .map_err(|e| fail(e.into()))?
            .map_err(fail)?
    };
    let Some(frame) = frame else {
        return Ok(());
    };
    let task_id = frame.task.as_ref().map(|t| t.id.clone());
    let request = json!({ "type": "message", "message": frame });
    let result = match app.peers.request(&peer.id, request).await {
        Ok(result) => result,
        Err(PeerError::Offline) => return Err(ForwardError::Offline),
        Err(PeerError::Rejected(reason)) => {
            return Err(fail(anyhow::anyhow!("{} refused it: {reason}", peer.name)))
        }
    };
    let received: Received = serde_json::from_value(result).map_err(|e| fail(e.into()))?;
    app.db
        .map_peer_message(&peer.id, &received.message_id, &msg.id)
        .map_err(fail)?;
    if let (Some(local), Some(remote)) = (task_id, received.task_id) {
        app.db
            .map_peer_task(&peer.id, &remote, &local)
            .map_err(fail)?;
    }
    Ok(())
}

/// The frame for `msg`, or `None` when it stays on this daemon.
fn build(
    app: &Arc<AppState>,
    peer: &Peer,
    target: &Bot,
    msg: &Message,
) -> anyhow::Result<Option<MessageFrame>> {
    let remote_bot_id = target
        .remote_bot_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("linked bot has no remote id"))?;
    let mut sender_bot = None;
    let from = match msg.sender.kind {
        // The daemon's own notices (expiries, renames, introductions) describe
        // this daemon's view. The peer produces its own for its side.
        SenderKind::User if msg.sender.name == "system" => return Ok(None),
        SenderKind::User => FromFrame::User,
        SenderKind::Bot => {
            let id = msg.sender.bot_id.as_deref().unwrap_or_default();
            let bot = app
                .db
                .get_bot(id)?
                .ok_or_else(|| anyhow::anyhow!("sender {id} missing"))?;
            anyhow::ensure!(
                !bot.is_linked(),
                "{} runs on another peer; messages are not relayed between peers",
                bot.name
            );
            // The recipient's answers come back addressed to the sender, so
            // the sender has to accept deliveries from this peer.
            app.db.expose_bot_to_peer(&peer.id, &bot.id)?;
            let view = super::inbound::remote_view(&bot, String::new());
            sender_bot = Some(bot);
            FromFrame::Bot { bot: view }
        }
        SenderKind::Routine => anyhow::bail!("routine prompts are never forwarded"),
    };

    let ref_message_id = match &msg.ref_message_id {
        Some(r) => Some(
            app.db
                .remote_message_for(&peer.id, r)?
                .unwrap_or_else(|| r.clone()),
        ),
        None => None,
    };

    let task = match msg.kind {
        MessageKind::Task => {
            let task = app
                .db
                .open_task_for_message(&msg.id, &target.id)?
                .ok_or_else(|| anyhow::anyhow!("task for message {} is no longer open", msg.num))?;
            Some(TaskFrame {
                id: task.id,
                deadline_at: task.deadline_at,
                hop_count: task.hop_count,
            })
        }
        _ => None,
    };

    let closes_task = closes(app, peer, target, msg)?;
    let artifacts = match (&sender_bot, msg.kind) {
        (Some(bot), MessageKind::Done) => super::artifacts::collect(app, bot, &msg.body),
        _ => Vec::new(),
    };

    Ok(Some(MessageFrame {
        id: msg.id.clone(),
        to_bot_id: remote_bot_id,
        from,
        kind: msg.kind,
        body: msg.body.clone(),
        ref_message_id,
        task,
        closes_task,
        artifacts,
    }))
}

/// The peer's half of the task this message closes: a result for a task the
/// linked bot delegated here, or the cancellation of one delegated to it.
fn closes(
    app: &Arc<AppState>,
    peer: &Peer,
    target: &Bot,
    msg: &Message,
) -> anyhow::Result<Option<ClosesFrame>> {
    let Some(origin) = &msg.ref_message_id else {
        return Ok(None);
    };
    let tasks = app.db.tasks_with_origin(origin)?;
    let found = match msg.kind {
        MessageKind::Done => tasks
            .iter()
            .find(|t| t.from_bot_id.as_deref() == Some(target.id.as_str()))
            .map(|t| (t, TaskState::Done)),
        MessageKind::Note => tasks
            .iter()
            .find(|t| t.to_bot_id == target.id && t.state == TaskState::Cancelled)
            .map(|t| (t, TaskState::Cancelled)),
        _ => None,
    };
    let Some((task, state)) = found else {
        anyhow::ensure!(
            msg.kind != MessageKind::Done,
            "result for a task {} did not delegate",
            target.name
        );
        return Ok(None);
    };
    match app.db.remote_task_for(&peer.id, &task.id)? {
        Some(remote) => Ok(Some(ClosesFrame { id: remote, state })),
        // A task cancelled before it ever reached the peer has nothing there
        // to close; the note still tells the bot to stop.
        None if state == TaskState::Cancelled => Ok(None),
        None => anyhow::bail!("task {} has no counterpart on {}", task.id, peer.name),
    }
}
