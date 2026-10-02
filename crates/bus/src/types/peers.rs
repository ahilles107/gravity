//! Peer daemons: another Gravity daemon of the same owner whose bots this
//! one can exchange messages with. See `docs/peer-bots.md`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Id;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub id: Id,
    /// The owner's name for the other machine, e.g. `win-pc`.
    pub name: String,
    /// The other daemon's stable id, learned at the first handshake. A token
    /// is bound to the first daemon that presents it.
    pub daemon_id: Option<String>,
    /// Where to dial, on the side that dials. `None` on the side that listens.
    pub url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Largest file sent with a forwarded result.
pub const MAX_PEER_ARTIFACT_BYTES: u64 = 16 * 1024 * 1024;

/// Largest total of files sent with one forwarded result.
pub const MAX_PEER_ARTIFACTS_TOTAL_BYTES: u64 = 48 * 1024 * 1024;

/// A bot as a peer describes it: enough to stand it in as a linked bot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteBot {
    pub id: Id,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub avatar: String,
    #[serde(default)]
    pub runtime: super::BotRuntime,
    /// The project it works in on its own daemon. Informational only.
    #[serde(default)]
    pub project: String,
    /// A temporary worker, archived on its machine once its task closes.
    #[serde(default)]
    pub temporary: bool,
}

/// A project here linked with a project on a peer: one team across two
/// machines. Recorded on both daemons, each with the other's project id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLink {
    pub project_id: Id,
    pub peer_id: Id,
    pub remote_project_id: Id,
    pub remote_project_name: String,
    pub linked_at: DateTime<Utc>,
}
