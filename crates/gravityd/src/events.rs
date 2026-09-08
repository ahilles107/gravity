//! Internal event bus: server pushes fanned out to all connected clients and
//! consumed internally (routine-run completion, activity previews).

use bus::{Bot, BotState, Delivery, Message, Project, RoutineRun};
use serde::Serialize;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Push {
    BotState {
        bot_id: String,
        state: BotState,
        reason: String,
        at: String,
    },
    MessageNew {
        message: Message,
    },
    /// A bot's identity changed, or it was created or archived. Bots edit
    /// themselves without asking, so open clients need this to avoid showing a
    /// stale name or avatar.
    BotUpdated {
        bot: Bot,
    },
    /// A project was created, renamed, or archived. Archival is signalled by
    /// `deleted_at` being set, exactly as it is for `BotUpdated`.
    ProjectUpdated {
        project: Project,
    },
    /// A bot's sidebar preview line changed. Emitted once the finished turn is
    /// actually readable in the transcript, which lags the `ready` state.
    ActivityUpdate {
        activity: crate::activity::BotActivity,
    },
    DeliveryUpdate {
        delivery: Delivery,
    },
    RoutineRunUpdate {
        routine_run: RoutineRun,
    },
    ApprovalPending {
        bot_id: String,
        detail: String,
    },
    Notify {
        level: String,
        title: String,
        body: String,
    },
}

/// Internal-only events that are not client pushes.
#[derive(Debug, Clone)]
pub enum Internal {
    /// A bot finished a turn. The Stop hook supplies the transcript used to
    /// correlate routine completion to one exact occurrence.
    BotDone {
        bot_id: String,
        transcript_path: Option<String>,
    },
}

#[derive(Clone)]
pub struct Events {
    push_tx: broadcast::Sender<Push>,
    internal_tx: broadcast::Sender<Internal>,
}

impl Events {
    pub fn new() -> Self {
        let (push_tx, _) = broadcast::channel(4096);
        let (internal_tx, _) = broadcast::channel(4096);
        Self {
            push_tx,
            internal_tx,
        }
    }

    pub fn push(&self, p: Push) {
        let _ = self.push_tx.send(p);
    }

    pub fn subscribe_push(&self) -> broadcast::Receiver<Push> {
        self.push_tx.subscribe()
    }

    pub fn internal(&self, e: Internal) {
        let _ = self.internal_tx.send(e);
    }

    pub fn subscribe_internal(&self) -> broadcast::Receiver<Internal> {
        self.internal_tx.subscribe()
    }
}

impl Default for Events {
    fn default() -> Self {
        Self::new()
    }
}
