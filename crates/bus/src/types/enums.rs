//! Lifecycle and trigger enums, with their wire spellings.

use serde::{Deserialize, Serialize};

use super::Id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BotState {
    Starting,
    Ready,
    Working,
    WaitingForUser,
    WaitingForApproval,
    RateLimited,
    AuthFailed,
    Crashed,
    Stopping,
    Stopped,
}

impl BotState {
    pub fn as_str(self) -> &'static str {
        match self {
            BotState::Starting => "starting",
            BotState::Ready => "ready",
            BotState::Working => "working",
            BotState::WaitingForUser => "waiting_for_user",
            BotState::WaitingForApproval => "waiting_for_approval",
            BotState::RateLimited => "rate_limited",
            BotState::AuthFailed => "auth_failed",
            BotState::Crashed => "crashed",
            BotState::Stopping => "stopping",
            BotState::Stopped => "stopped",
        }
    }

    pub fn is_running(self) -> bool {
        !matches!(
            self,
            BotState::Stopped | BotState::Crashed | BotState::AuthFailed
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Queued,
    Leased,
    Delivered,
    Acknowledged,
    Failed,
}

impl DeliveryState {
    pub fn as_str(self) -> &'static str {
        match self {
            DeliveryState::Queued => "queued",
            DeliveryState::Leased => "leased",
            DeliveryState::Delivered => "delivered",
            DeliveryState::Acknowledged => "acknowledged",
            DeliveryState::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(DeliveryState::Queued),
            "leased" => Some(DeliveryState::Leased),
            "delivered" => Some(DeliveryState::Delivered),
            "acknowledged" => Some(DeliveryState::Acknowledged),
            "failed" => Some(DeliveryState::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Task,
    Reply,
    Done,
    Note,
    Chat,
}

impl MessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageKind::Task => "task",
            MessageKind::Reply => "reply",
            MessageKind::Done => "done",
            MessageKind::Note => "note",
            MessageKind::Chat => "chat",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "task" => Some(MessageKind::Task),
            "reply" => Some(MessageKind::Reply),
            "done" => Some(MessageKind::Done),
            "note" => Some(MessageKind::Note),
            "chat" => Some(MessageKind::Chat),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlapPolicy {
    Skip,
    QueueOne,
    QueueAll,
    Replace,
}

impl OverlapPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            OverlapPolicy::Skip => "skip",
            OverlapPolicy::QueueOne => "queue_one",
            OverlapPolicy::QueueAll => "queue_all",
            OverlapPolicy::Replace => "replace",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "skip" => Some(OverlapPolicy::Skip),
            "queue_one" => Some(OverlapPolicy::QueueOne),
            "queue_all" => Some(OverlapPolicy::QueueAll),
            "replace" => Some(OverlapPolicy::Replace),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineRunState {
    Scheduled,
    Running,
    Succeeded,
    Failed,
    Skipped,
    Cancelled,
}

impl RoutineRunState {
    pub fn as_str(self) -> &'static str {
        match self {
            RoutineRunState::Scheduled => "scheduled",
            RoutineRunState::Running => "running",
            RoutineRunState::Succeeded => "succeeded",
            RoutineRunState::Failed => "failed",
            RoutineRunState::Skipped => "skipped",
            RoutineRunState::Cancelled => "cancelled",
        }
    }
}

/// What causes a routine to produce an occurrence. `Signal` is the single
/// extension point for non-time triggers, so new sources add a signal producer
/// rather than a variant here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    Cron {
        expr: String,
        #[serde(default = "default_tz")]
        tz: String,
    },
    Interval {
        seconds: u64,
    },
    Signal {
        name: String,
        /// Omitted means any emitter in the owning bot's project.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_bot_id: Option<Id>,
    },
}

impl Trigger {
    pub fn is_time_based(&self) -> bool {
        matches!(self, Trigger::Cron { .. } | Trigger::Interval { .. })
    }

    /// Stand-in for a trigger that no longer parses: an interval past chrono's
    /// safe range, so `next_occurrence` yields `None` and it never fires.
    pub fn unparsable() -> Self {
        Trigger::Interval { seconds: u64::MAX }
    }
}

fn default_tz() -> String {
    "UTC".to_string()
}

/// Where a signal came from. New producers are added here, not to `Trigger`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalSource {
    Bot,
    Manual,
}

impl SignalSource {
    pub fn as_str(self) -> &'static str {
        match self {
            SignalSource::Bot => "bot",
            SignalSource::Manual => "manual",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "bot" => Some(SignalSource::Bot),
            "manual" => Some(SignalSource::Manual),
            _ => None,
        }
    }
}

/// Why an occurrence exists, so history can answer that without re-deriving
/// it from the trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunSource {
    Schedule,
    Manual,
    Signal,
}

impl RunSource {
    pub fn as_str(self) -> &'static str {
        match self {
            RunSource::Schedule => "schedule",
            RunSource::Manual => "manual",
            RunSource::Signal => "signal",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "schedule" => Some(RunSource::Schedule),
            "manual" => Some(RunSource::Manual),
            "signal" => Some(RunSource::Signal),
            _ => None,
        }
    }
}
