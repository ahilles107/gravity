//! Internal event bus: server pushes fanned out to all connected clients and
//! consumed internally (routine-run completion, activity previews).

use bus::{Bot, BotState, DecisionComment, DecisionView, Delivery, Message, Project, RoutineRun};
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
    /// A decision was raised, edited, answered, held, published or withdrawn.
    /// Boxed because a decision carries its body, and every other variant
    /// would otherwise pay for the largest one.
    DecisionUpdate {
        decision: Box<DecisionView>,
    },
    DecisionDeleted {
        decision_id: String,
    },
    DecisionCommentNew {
        comment: DecisionComment,
    },
    ApprovalPending {
        bot_id: String,
        detail: String,
    },
    Notify {
        level: String,
        title: String,
        body: String,
        /// Set when the notice is about a decision, so the client can raise a
        /// native notification that opens the record rather than a toast the
        /// owner has to go looking behind.
        #[serde(skip_serializing_if = "Option::is_none")]
        decision_id: Option<String>,
    },
}

impl Push {
    /// A toast: something happened the user should know about but need not act
    /// on. `botmgmt` says it best — "a toast, not a prompt".
    pub fn notice(level: &str, title: impl Into<String>, body: impl Into<String>) -> Self {
        Push::Notify {
            level: level.to_string(),
            title: title.into(),
            body: body.into(),
            decision_id: None,
        }
    }

    /// A notice about a decision. The client may raise this natively, because
    /// an urgent ask or a deadline inside a day is exactly what the owner
    /// cannot afford to find out about later.
    pub fn decision_notice(
        title: impl Into<String>,
        body: impl Into<String>,
        decision_id: &str,
    ) -> Self {
        Push::Notify {
            level: "warn".to_string(),
            title: title.into(),
            body: body.into(),
            decision_id: Some(decision_id.to_string()),
        }
    }
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
