//! Persisting and delivering messages on the bus. Every conversation is a
//! bot's DM thread: one message row, one task, one delivery per send.

use anyhow::Context;
use bus::{Message, MessageKind, Sender, SenderKind};

use crate::db::Db;
use crate::events::{Events, Push};

/// One direct message, described rather than passed as a run of arguments.
///
/// The correlation fields are optional and rarely both set, which is exactly
/// the shape positional parameters render unreadable.
pub struct Dm<'a> {
    pub bot_id: &'a str,
    pub sender: &'a Sender,
    pub kind: MessageKind,
    pub body: &'a str,
    /// The message this answers, when there is one.
    pub ref_message_id: Option<&'a str>,
    /// The decision this is about. Set on ruling, comment and hold notices, so
    /// the envelope renderer and the client can link back to the record.
    pub decision_id: Option<&'a str>,
}

impl<'a> Dm<'a> {
    pub fn new(bot_id: &'a str, sender: &'a Sender, kind: MessageKind, body: &'a str) -> Self {
        Self {
            bot_id,
            sender,
            kind,
            body,
            ref_message_id: None,
            decision_id: None,
        }
    }

    pub fn re(mut self, ref_message_id: &'a str) -> Self {
        self.ref_message_id = Some(ref_message_id);
        self
    }

    pub fn about(mut self, decision_id: &'a str) -> Self {
        self.decision_id = Some(decision_id);
        self
    }
}

/// Persist and deliver a direct message to one bot's DM conversation.
pub fn send_dm(db: &Db, events: &Events, dm: Dm<'_>) -> anyhow::Result<Message> {
    let conv = db
        .dm_conversation(dm.bot_id)?
        .context("bot has no DM conversation")?;
    let msg = db.insert_message(
        &conv.id,
        dm.sender,
        dm.kind,
        dm.body,
        dm.ref_message_id,
        dm.decision_id,
    )?;
    events.push(Push::MessageNew {
        message: msg.clone(),
    });
    let key = format!("{}:{}", msg.id, dm.bot_id);
    let delivery = db.enqueue_delivery(&msg.id, dm.bot_id, &key)?;
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
