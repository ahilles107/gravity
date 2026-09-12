//! The decision registry's records.
//!
//! `Decision` is the stored row. `DecisionView` is what every caller — the
//! control plane, MCP, and a future REST route — actually receives: the row
//! plus the things that live in other tables (tags, the raiser's name, the
//! thread). Keeping those apart is what stops a reply and a push from drifting.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{CommentAuthorKind, DecisionKind, DecisionState, Id, Priority};

/// One choice the owner can take, as the raising bot framed it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionOption {
    pub key: String,
    pub label: String,
    /// What happens if this one is picked — the blast radius, in the bot's
    /// words. Optional, but a recommendation without one is hard to trust.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The owner's answer, once there is one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ruling {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<String>,
    /// The owner's words. Verbatim is the norm; bots quote it.
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub answered_at: DateTime<Utc>,
    /// `owner`, `device:<id>`, or `owner-via-bot:<bot id>` when a bot filed a
    /// ruling the owner gave it at its own terminal. Bot names are mutable, so
    /// the id is stored and the name resolved for display.
    pub answered_by: String,
}

/// A durable record of something only the owner can settle.
#[derive(Debug, Clone)]
pub struct Decision {
    pub id: Id,
    pub project_id: Id,
    pub kind: DecisionKind,
    pub title: String,
    /// Lowercased, punctuation-stripped title. Only exists so "you already
    /// have this one open" is an indexed lookup.
    pub normalised_title: String,
    pub body: String,
    pub options: Vec<DecisionOption>,
    pub recommendation: Option<String>,
    pub raised_by_bot_id: Id,
    /// The bot whose work waits on this, when a lead raised it for a teammate.
    pub on_behalf_of_bot_id: Option<Id>,
    pub origin_chain: String,
    pub source_message_id: Option<Id>,
    pub source_task_id: Option<Id>,
    pub priority: Priority,
    pub deadline_at: Option<DateTime<Utc>>,
    pub deadline_notified_at: Option<DateTime<Utc>>,
    pub state: DecisionState,
    pub held_until: Option<DateTime<Utc>>,
    pub ruling: Option<Ruling>,
    pub published_at: Option<DateTime<Utc>>,
    pub supersedes_id: Option<Id>,
    pub superseded_by_id: Option<Id>,
    pub withdrawn_reason: Option<String>,
    pub edited_at: Option<DateTime<Utc>>,
    pub edited_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// One turn in a decision's thread. This is the clarification channel: an
/// owner comment reaches the asker as a bus note, and the asker answers here
/// rather than in a DM, so the whole exchange stays on the record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionComment {
    pub id: Id,
    pub decision_id: Id,
    pub author_kind: CommentAuthorKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_bot_id: Option<Id>,
    /// Display name at render time, not at write time.
    pub author_name: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

/// A category shared by every project, so a bot filing `spend` in one means
/// what a bot filing `spend` in another does.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: Id,
    pub name: String,
    pub description: String,
    pub color: String,
    pub created_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// A tag with how many decisions use it, per project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagUsage {
    #[serde(flatten)]
    pub tag: Tag,
    pub uses: std::collections::BTreeMap<String, i64>,
    /// When a decision was last filed under it, settled or raised.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    /// How many of its decisions are still the owner's to deal with.
    pub open_uses: i64,
}

/// Which bot was told about a settled decision, and with what delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionNotification {
    pub bot_id: Id,
    pub bot_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_id: Option<Id>,
    pub created_at: DateTime<Utc>,
}

/// A decision as every transport serialises it.
///
/// `comments` and `notifications` are empty in list responses and filled in
/// for a single record; `comment_count` is always accurate, so the client can
/// show an unread marker without fetching the thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionView {
    pub id: Id,
    pub project_id: Id,
    pub kind: DecisionKind,
    pub title: String,
    pub body: String,
    pub options: Vec<DecisionOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<String>,
    pub raised_by: RaisedBy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_behalf_of_bot_id: Option<Id>,
    pub origin_chain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_message_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_task_id: Option<Id>,
    pub priority: Priority,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_at: Option<DateTime<Utc>>,
    pub state: DecisionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub held_until: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ruling: Option<Ruling>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub withdrawn_reason: Option<String>,
    pub tags: Vec<String>,
    pub comment_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_comment_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<DecisionComment>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notifications: Vec<DecisionNotification>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Who raised a decision, named rather than just identified — the owner should
/// not have to look up an id to know who is waiting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaisedBy {
    pub bot_id: Id,
    pub name: String,
    pub avatar: String,
}

/// How many decisions are waiting, for the sidebar and the dock badge.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PendingCounts {
    pub by_project: std::collections::BTreeMap<String, i64>,
    pub total: i64,
    pub urgent: i64,
    pub due_soon: i64,
}
