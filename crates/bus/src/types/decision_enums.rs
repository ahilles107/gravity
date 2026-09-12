//! States and classifications for the decision registry.

use serde::{Deserialize, Serialize};

/// What the owner is being asked for. `Question` expects free text; `Decision`
/// carries options. Both are answered the same way — the distinction is there
/// so the client can render a recommendation list rather than an empty box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    Question,
    Decision,
}

impl DecisionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionKind::Question => "question",
            DecisionKind::Decision => "decision",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "question" => Some(DecisionKind::Question),
            "decision" => Some(DecisionKind::Decision),
            _ => None,
        }
    }
}

/// Where a decision sits between being raised and being authority.
///
/// `Answered` exists so the owner can work through a batch and publish once;
/// bots do not see a ruling until it is `Settled`. `Held` is the owner saying
/// "later" out loud — the alternative, which the live projects used, was
/// carrying held questions forward in prompt text forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionState {
    Open,
    Answered,
    Held,
    Settled,
    Withdrawn,
}

impl DecisionState {
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionState::Open => "open",
            DecisionState::Answered => "answered",
            DecisionState::Held => "held",
            DecisionState::Settled => "settled",
            DecisionState::Withdrawn => "withdrawn",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "open" => Some(DecisionState::Open),
            "answered" => Some(DecisionState::Answered),
            "held" => Some(DecisionState::Held),
            "settled" => Some(DecisionState::Settled),
            "withdrawn" => Some(DecisionState::Withdrawn),
            _ => None,
        }
    }

    /// True while the decision is still the owner's to deal with. This is what
    /// the badge counts: a draft ruling the owner has not published is still
    /// work they owe, while a held item is one they have consciously parked.
    pub fn is_pending(self) -> bool {
        matches!(self, DecisionState::Open | DecisionState::Answered)
    }

    /// True while a bot may still withdraw or the owner may still edit freely.
    pub fn is_before_publish(self) -> bool {
        matches!(
            self,
            DecisionState::Open | DecisionState::Answered | DecisionState::Held
        )
    }
}

/// How loudly a decision asks. Set by the raising bot; drives the badge colour
/// and a one-off notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Normal,
    Urgent,
}

impl Priority {
    pub fn as_str(self) -> &'static str {
        match self {
            Priority::Normal => "normal",
            Priority::Urgent => "urgent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "normal" => Some(Priority::Normal),
            "urgent" => Some(Priority::Urgent),
            _ => None,
        }
    }
}

/// Who wrote a comment on a decision thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentAuthorKind {
    Bot,
    User,
}

impl CommentAuthorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CommentAuthorKind::Bot => "bot",
            CommentAuthorKind::User => "user",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "bot" => Some(CommentAuthorKind::Bot),
            "user" => Some(CommentAuthorKind::User),
            _ => None,
        }
    }
}
