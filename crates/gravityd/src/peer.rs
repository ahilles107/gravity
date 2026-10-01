//! Peer daemons: bots on two of the owner's machines working as one team.
//!
//! A bot that runs on a peer stands in here as a linked bot. Sending to it is
//! sending to any bot; the delivery worker forwards the message over the peer
//! link instead of writing it to a local session, and the peer stores it as if
//! the sender were one of its own. See `docs/peer-bots.md`.

mod artifacts;
pub mod chat;
pub mod cli;
mod forward;
mod frames;
mod hub;
mod inbound;
pub mod links;
pub mod mirror;
mod receive;
pub mod remote_bots;
mod roster;
mod socket;

pub use forward::{forward, ForwardError};
pub use hub::{PeerError, PeerHub};
pub use socket::{peer_handler, spawn_dialer, spawn_dialers};

/// A refusal with a code a client can act on. Raised on either side of a
/// peer link: a peer's refusal crosses the link with its code, so the owner's
/// client sees `conflict` or `not_linked` whichever daemon decided it.
#[derive(Debug)]
pub struct Refusal {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Refusal {}

/// An error carrying `code` to the client.
pub fn refuse(code: &'static str, message: impl Into<String>) -> anyhow::Error {
    Refusal {
        code,
        message: message.into(),
    }
    .into()
}

/// The code a client should see for an error: a [`Refusal`]'s own, a peer's
/// when the error came from one, otherwise `fallback`.
pub fn error_code<'a>(error: &'a anyhow::Error, fallback: &'a str) -> &'a str {
    if let Some(refusal) = error.downcast_ref::<Refusal>() {
        refusal.code
    } else if let Some(peer) = error.downcast_ref::<PeerError>() {
        peer.code()
    } else {
        fallback
    }
}
