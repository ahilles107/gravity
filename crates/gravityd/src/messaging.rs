//! Persisting and delivering messages on the bus. Every conversation is a
//! bot's DM thread: one message row, one task, one delivery per send.

use anyhow::Context;
use bus::{Message, MessageKind, Sender, SenderKind};

use crate::db::Db;
use crate::events::{Events, Push};

/// Persist and deliver a direct message to one bot's DM conversation.
pub fn send_dm(
    db: &Db,
    events: &Events,
    bot_id: &str,
    sender: &Sender,
    kind: MessageKind,
    body: &str,
    ref_message_id: Option<&str>,
) -> anyhow::Result<Message> {
    let conv = db
        .dm_conversation(bot_id)?
        .context("bot has no DM conversation")?;
    let msg = db.insert_message(&conv.id, sender, kind, body, ref_message_id)?;
    events.push(Push::MessageNew {
        message: msg.clone(),
    });
    let key = format!("{}:{}", msg.id, bot_id);
    let delivery = db.enqueue_delivery(&msg.id, bot_id, &key)?;
    events.push(Push::DeliveryUpdate { delivery });
    Ok(msg)
}

/// True when the sender is the user (not subject to hop limits).
pub fn user_sender() -> Sender {
    Sender {
        kind: SenderKind::User,
        bot_id: None,
        name: "user".to_string(),
    }
}

/// The daemon speaking for itself: introductions, rename announcements,
/// released and expired tasks.
pub fn daemon_sender() -> Sender {
    Sender {
        kind: SenderKind::User,
        bot_id: None,
        name: "system".to_string(),
    }
}
