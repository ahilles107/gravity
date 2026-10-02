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
mod receive;
mod socket;

pub use forward::{forward, ForwardError};
pub use hub::{PeerError, PeerHub};
pub use socket::{peer_handler, spawn_dialer, spawn_dialers};
