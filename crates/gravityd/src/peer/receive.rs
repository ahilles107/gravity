//! Storing a message a peer forwarded: the receiving half of the delivery
//! worker's forward. Each step mirrors what the local tool would have done had
//! the sender been here, so the recipient cannot tell the difference beyond
//! the machine named in the envelope.

use std::sync::Arc;

use bus::{MessageKind, Peer, RemoteBot, Sender, SenderKind, TaskState};

use crate::app::AppState;
use crate::events::Push;
use crate::messaging::{self, Dm};

use super::frames::{FromFrame, MessageFrame, Received};

pub(super) fn receive(
    app: &Arc<AppState>,
    peer: &Peer,
    frame: MessageFrame,
) -> anyhow::Result<Received> {
    // At-least-once on the wire: an ack lost to a dropped link means the same
    // frame again. The first copy's result is the answer.
    if let Some(local) = app.db.local_message_for(&peer.id, &frame.id)? {
        let task_id = app
            .db
            .tasks_with_origin(&local)?
            .into_iter()
            .next()
            .map(|t| t.id);
        return Ok(Received {
            message_id: local,
            task_id,
        });
    }

    let to = app
        .db
        .get_live_bot(&frame.to_bot_id)?
        .filter(|b| !b.is_linked())
        .ok_or_else(|| anyhow::anyhow!("no bot with id {} runs here", frame.to_bot_id))?;
    anyhow::ensure!(
        app.db.is_exposed_to_peer(&peer.id, &to.id)?,
        "{} is not linked to this daemon",
        to.name
    );

    let (sender, linked) = match &frame.from {
        FromFrame::User => (messaging::user_sender(), None),
        FromFrame::Bot { bot } => {
            anyhow::ensure!(
                frame.kind != MessageKind::Chat,
                "only the owner sends chat messages"
            );
            let linked = linked_sender(app, peer, bot, &to.project_id)?;
            let sender = Sender {
                kind: SenderKind::Bot,
                bot_id: Some(linked.id.clone()),
                name: linked.name.clone(),
            };
            (sender, Some(linked))
        }
    };
    let ref_id = match &frame.ref_message_id {
        Some(r) => match app.db.local_message_for(&peer.id, r)? {
            Some(local) => Some(local),
            None => app.db.get_message(r)?.map(|m| m.id),
        },
        None => None,
    };
    let body = if frame.artifacts.is_empty() {
        frame.body.clone()
    } else {
        super::artifacts::store(app, peer, &to, &frame)?
    };

    let msg = match &frame.closes_task {
        Some(closes) => {
            let linked = linked
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("only a bot closes a task"))?;
            let task = app
                .db
                .get_task(&closes.id)?
                .ok_or_else(|| anyhow::anyhow!("no task {} here", closes.id))?;
            match closes.state {
                TaskState::Done => {
                    anyhow::ensure!(
                        task.to_bot_id == linked.id
                            && task.from_bot_id.as_deref() == Some(to.id.as_str()),
                        "task {} is not one {} was given by {}",
                        task.id,
                        linked.name,
                        to.name
                    );
                    // Already closed here (expired while the result was in
                    // flight) still stores the result: the work was done.
                    app.db.try_close_task(&task.id, TaskState::Done)?;
                    let origin = app
                        .db
                        .get_message(&task.origin_message_id)?
                        .ok_or_else(|| anyhow::anyhow!("origin message missing"))?;
                    let msg = app.db.insert_message(
                        &origin.conversation_id,
                        &sender,
                        MessageKind::Done,
                        &body,
                        Some(&origin.id),
                        None,
                    )?;
                    app.events.push(Push::MessageNew {
                        message: msg.clone(),
                    });
                    let key = format!("{}:{}", msg.id, to.id);
                    let delivery = app.db.enqueue_delivery(&msg.id, &to.id, &key)?;
                    app.events.push(Push::DeliveryUpdate { delivery });
                    msg
                }
                TaskState::Cancelled => {
                    anyhow::ensure!(
                        task.from_bot_id.as_deref() == Some(linked.id.as_str())
                            && task.to_bot_id == to.id,
                        "task {} is not one {} gave {}",
                        task.id,
                        linked.name,
                        to.name
                    );
                    app.db.try_close_task(&task.id, TaskState::Cancelled)?;
                    let mut dm = Dm::new(&to.id, &sender, frame.kind, &body);
                    dm.ref_message_id = ref_id.as_deref();
                    messaging::send_dm(&app.db, &app.events, dm)?
                }
                other => anyhow::bail!("a task cannot be closed as {}", other.as_str()),
            }
        }
        None => {
            anyhow::ensure!(
                frame.kind != MessageKind::Done,
                "a result must name the task it closes"
            );
            let mut dm = Dm::new(&to.id, &sender, frame.kind, &body);
            dm.ref_message_id = ref_id.as_deref();
            messaging::send_dm(&app.db, &app.events, dm)?
        }
    };

    // A delegation opens the same task here, so the recipient's
    // `complete_task` and every budget work as if the sender were local. The
    // hop count carries over, so the chain limit holds across machines.
    let mut task_id = None;
    if frame.kind == MessageKind::Task {
        let (Some(spec), Some(linked)) = (&frame.task, &linked) else {
            anyhow::bail!("a task needs a task frame and a bot sender");
        };
        let task = app.db.create_task(
            &msg.id,
            Some(&linked.id),
            &to.id,
            spec.deadline_at,
            spec.hop_count,
            &linked.id,
        )?;
        app.db.map_peer_task(&peer.id, &spec.id, &task.id)?;
        task_id = Some(task.id);
    }
    app.db.map_peer_message(&peer.id, &frame.id, &msg.id)?;
    Ok(Received {
        message_id: msg.id,
        task_id,
    })
}

/// The linked bot standing in for the peer's sender, created on first
/// contact. It takes the sender's name, or `<name>-<peer>` if a bot here
/// already has it.
fn linked_sender(
    app: &Arc<AppState>,
    peer: &Peer,
    remote: &RemoteBot,
    project_id: &str,
) -> anyhow::Result<bus::Bot> {
    if let Some(bot) = app.db.linked_bot(&peer.id, &remote.id, project_id)? {
        return Ok(bot);
    }
    let mut name = bus::names::validate(&remote.name).map_err(anyhow::Error::msg)?;
    if app.db.get_bot_by_name(project_id, &name)?.is_some() {
        name =
            bus::names::validate(&format!("{name}-{}", peer.name)).map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            app.db.get_bot_by_name(project_id, &name)?.is_none(),
            "both '{}' and '{name}' are taken in this project",
            remote.name
        );
    }
    let stand_in = RemoteBot {
        name,
        ..remote.clone()
    };
    let bot = app.db.create_linked_bot(project_id, &stand_in, &peer.id)?;
    tracing::info!(peer = %peer.name, bot = %bot.name, "linked bot created on first contact");
    app.events.push(Push::BotUpdated { bot: bot.clone() });
    Ok(bot)
}
