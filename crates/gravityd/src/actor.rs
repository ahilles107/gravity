//! Who is asking.
//!
//! The daemon serves the owner at a client, a scoped device credential, and a
//! bot over MCP. Every authority check and every audit row needs to name
//! which, and keeping that in one type is what lets the decision service
//! answer a WebSocket request and an MCP tool call with the same code.

/// Author of a change, as stored in `bot_revision.changed_by` and
/// `decision.answered_by`.
pub enum Actor<'a> {
    /// The owner, on the owner token.
    User,
    /// The owner, through a scoped device credential. Stored distinctly so a
    /// ruling stays attributable after the device is revoked.
    Device(&'a str),
    /// A bot over MCP. The project comes from the bearer token that
    /// authenticated the call, never from an argument, so it cannot be forged.
    Bot { id: &'a str, project_id: &'a str },
}

impl Actor<'_> {
    pub fn as_stored(&self) -> String {
        match self {
            Actor::User => "user".to_string(),
            Actor::Device(id) => format!("device:{id}"),
            Actor::Bot { id, .. } => format!("bot:{id}"),
        }
    }

    /// True for the human at a client, however they authenticated. Only the
    /// owner rules on a decision; a device is still the owner.
    pub fn is_owner(&self) -> bool {
        matches!(self, Actor::User | Actor::Device(_))
    }

    pub fn bot_id(&self) -> Option<&str> {
        match self {
            Actor::Bot { id, .. } => Some(id),
            _ => None,
        }
    }

    pub fn project_id(&self) -> Option<&str> {
        match self {
            Actor::Bot { project_id, .. } => Some(project_id),
            _ => None,
        }
    }
}
