//! Temporary workers: bots spawned for one task, held in a queue until a
//! worker slot is free.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{BotRuntime, Id};

/// Workers a project may run at once on one machine, unless configured.
pub const DEFAULT_MAX_WORKERS_PER_PROJECT: usize = 4;

/// Where a spawn is in its life. `queued` waits for a slot; `running` has a
/// bot and an open task; the rest are final and say how the task closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerState {
    Queued,
    Running,
    Done,
    Cancelled,
    Expired,
    /// Never started: placing it failed for a reason waiting will not fix.
    Failed,
}

impl WorkerState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
            Self::Failed => "failed",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            "expired" => Self::Expired,
            "failed" => Self::Failed,
            _ => return None,
        })
    }

    pub fn is_final(self) -> bool {
        !matches!(self, Self::Queued | Self::Running)
    }
}

/// One spawn, as the asking daemon tracks it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    pub id: Id,
    pub project_id: Id,
    /// The bot that asked for it, and that its task reports to.
    pub parent_bot_id: Id,
    /// Reserved while queued, so the parent can address it by this name.
    pub name: String,
    /// The task it is given when it starts.
    pub brief: String,
    pub description: String,
    pub instructions: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<BotRuntime>,
    /// The peer it must run on, by name, or `None` for any machine with a
    /// free slot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    pub deadline_hours: i64,
    pub state: WorkerState,
    /// The bot running it: local, or the stand-in for one on a peer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
}
