//! The decision registry service.
//!
//! Every rule about decisions lives here, and the three transports — the
//! control plane, MCP, and eventually a REST route — parse, call, and
//! serialise. That is not tidiness: the same authority question ("may this
//! caller withdraw this decision?") has to have the same answer whether it
//! arrives over a WebSocket or a bot's bearer token, and a rule duplicated in
//! two handlers is a rule that will eventually disagree with itself.

mod authority;
mod owner;
mod publish;
mod record;
mod service;
mod validate;
mod view;
mod watch;

pub use authority::{may, require_owner, Action};
pub use owner::*;
pub use publish::{publish, PublishOutcome};
pub use record::{record, Recorded};
pub use service::*;
pub use validate::{checked_color, checked_tag_description, checked_tags};
pub use view::{decision_view, decision_views, Detail};
pub use watch::run_watch;

/// What went wrong, in the vocabulary both transports already speak.
///
/// The WebSocket layer turns these into its own error codes and a future REST
/// router into HTTP statuses; neither has to invent a mapping, because the
/// service already decided which kind of failure this is.
#[derive(Debug)]
pub enum DecisionError {
    NotFound(String),
    Forbidden(String),
    Conflict(String),
    Invalid(String),
}

impl DecisionError {
    pub fn code(&self) -> &'static str {
        match self {
            DecisionError::NotFound(_) => "not_found",
            DecisionError::Forbidden(_) => "forbidden",
            DecisionError::Conflict(_) => "conflict",
            DecisionError::Invalid(_) => "invalid_request",
        }
    }
}

impl std::fmt::Display for DecisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecisionError::NotFound(m)
            | DecisionError::Forbidden(m)
            | DecisionError::Conflict(m)
            | DecisionError::Invalid(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for DecisionError {}

/// Pull a `DecisionError` back out of an `anyhow::Error` so a transport can
/// answer with the right code instead of a blanket `internal`.
pub fn error_code(error: &anyhow::Error) -> Option<&'static str> {
    error.downcast_ref::<DecisionError>().map(|e| e.code())
}

fn invalid(message: impl Into<String>) -> anyhow::Error {
    DecisionError::Invalid(message.into()).into()
}

fn forbidden(message: impl Into<String>) -> anyhow::Error {
    DecisionError::Forbidden(message.into()).into()
}

pub(crate) fn conflict(message: impl Into<String>) -> anyhow::Error {
    DecisionError::Conflict(message.into()).into()
}

pub(crate) fn not_found(message: impl Into<String>) -> anyhow::Error {
    DecisionError::NotFound(message.into()).into()
}
