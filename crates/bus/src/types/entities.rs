//! Entity records exchanged between the daemon, bots and clients.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{
    BotState, DeliveryState, Id, MessageKind, OverlapPolicy, RoutineRunState, RunSource,
    SignalSource, Trigger,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Id,
    pub name: String,
    /// Directory under `projects/`. Derived from the name at creation and then
    /// frozen, so renaming never moves the bots that live inside it.
    pub dir_name: String,
    /// Set when archived: the row survives so its bots stay attributable.
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bot {
    pub id: Id,
    pub project_id: Id,
    pub name: String,
    pub description: String,
    pub avatar: String,
    pub instructions: String,
    pub state: BotState,
    pub state_reason: String,
    pub unread_count: i64,
    pub workspace_path: String,
    /// Directory under `projects/<project>/bots/`. Derived from the name at
    /// creation and then frozen, so renaming never moves a live workspace.
    pub dir_name: String,
    /// The bot that created this one. Provenance, not ownership: it grants the
    /// creator edit and delete rights, but an archived creator leaves its
    /// children running and user-managed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_bot_id: Option<Id>,
    /// Set when the bot has been archived. Archived bots keep their history but
    /// leave `list_bots`, addressing, and the population cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// One field-level change to a bot's identity: the audit trail that makes
/// unsupervised self-management reversible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotRevision {
    pub id: Id,
    pub bot_id: Id,
    /// `user` or `bot:<id>`.
    pub changed_by: String,
    pub field: RevisionField,
    pub old_value: String,
    pub new_value: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionField {
    Name,
    Avatar,
    Description,
    Instructions,
    /// Lifecycle markers rather than field edits; `new_value` carries the
    /// reason, so a bot's whole life is one ordered list.
    Created,
    Deleted,
}

impl RevisionField {
    pub fn as_str(self) -> &'static str {
        match self {
            RevisionField::Name => "name",
            RevisionField::Avatar => "avatar",
            RevisionField::Description => "description",
            RevisionField::Instructions => "instructions",
            RevisionField::Created => "created",
            RevisionField::Deleted => "deleted",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "name" => Some(RevisionField::Name),
            "avatar" => Some(RevisionField::Avatar),
            "description" => Some(RevisionField::Description),
            "instructions" => Some(RevisionField::Instructions),
            "created" => Some(RevisionField::Created),
            "deleted" => Some(RevisionField::Deleted),
            _ => None,
        }
    }

    /// Whether reverting this entry means writing `old_value` back. Lifecycle
    /// markers are history, not state, so they are not revertible.
    pub fn is_revertible(self) -> bool {
        !matches!(self, RevisionField::Created | RevisionField::Deleted)
    }
}

/// A bot's DM thread. Every bot has exactly one, created with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Id,
    pub project_id: Id,
    pub bot_id: Id,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SenderKind {
    User,
    Bot,
    Routine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sender {
    pub kind: SenderKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<Id>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Id,
    /// Monotonic per-database number used for human-readable references.
    pub num: i64,
    pub conversation_id: Id,
    pub sender: Sender,
    pub kind: MessageKind,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_message_id: Option<Id>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delivery {
    pub id: Id,
    pub message_id: Id,
    pub bot_id: Id,
    pub state: DeliveryState,
    pub attempt_count: i64,
    pub next_attempt_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    pub id: Id,
    pub bot_id: Id,
    pub name: String,
    pub trigger: Trigger,
    pub prompt: String,
    pub overlap_policy: OverlapPolicy,
    pub enabled: bool,
    /// `None` falls back to the daemon-wide scheduler lease.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_duration_seconds: Option<i64>,
    /// Attempts per occurrence, including the first. `1` never retries.
    pub max_attempts: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// One occurrence of a routine. `delivery_id` is what makes completion exact:
/// a run finishes on a turn only once its own prompt reached the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineRun {
    pub id: Id,
    pub routine_id: Id,
    pub scheduled_for: DateTime<Utc>,
    pub state: RoutineRunState,
    pub source: RunSource,
    pub attempt: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_attempt_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A named thing that happened, which routines subscribe to by name. One
/// table serves every non-time trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub id: Id,
    pub name: String,
    pub source: SignalSource,
    /// Routines outside this project never see it.
    pub project_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_bot_id: Option<Id>,
    pub payload: serde_json::Value,
    /// Comma-separated bot ids the signal passed through, oldest first. A
    /// routine whose owner appears here is skipped, which breaks cycles.
    pub origin_chain: String,
    pub hop_count: i64,
    pub emitted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Id,
    pub origin_message_id: Id,
    pub from_bot_id: Option<Id>,
    pub to_bot_id: Id,
    pub state: TaskState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_at: Option<DateTime<Utc>>,
    pub hop_count: i64,
    /// Comma-separated chain of bot ids from the origin, used for loop prevention.
    pub origin_chain: String,
    /// Replies exchanged on this task, both directions; capped at `MAX_TASK_REPLIES`.
    pub reply_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Open,
    Done,
    Cancelled,
    Expired,
}

impl TaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskState::Open => "open",
            TaskState::Done => "done",
            TaskState::Cancelled => "cancelled",
            TaskState::Expired => "expired",
        }
    }
}

/// Maximum hops a task chain may take before the daemon refuses further
/// bot-to-bot sends; prevents delegation loops.
pub const MAX_TASK_HOPS: i64 = 4;

/// Maximum replies exchanged on one task (both directions share the budget);
/// past it only `complete_task` moves the work forward.
pub const MAX_TASK_REPLIES: i64 = 6;

/// Maximum open tasks a bot may have delegated from one chain position.
pub const MAX_TASK_FANOUT: i64 = 3;

/// Deadline applied to a bot-to-bot task when the sender sets none.
pub const DEFAULT_TASK_DEADLINE_HOURS: i64 = 24;

/// Maximum inbound message body size accepted from bots or events.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// Capability grants carried by a device credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// List, attach read-only, search, diagnostics.
    Read,
    /// Everything: input, leases, CRUD, messaging, routines.
    Control,
    /// Reserved for the future custom approval bridge.
    Approve,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Read => "read",
            Capability::Control => "control",
            Capability::Approve => "approve",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "read" => Some(Capability::Read),
            "control" => Some(Capability::Control),
            "approve" => Some(Capability::Approve),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: Id,
    pub name: String,
    pub capabilities: Vec<Capability>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<DateTime<Utc>>,
}
