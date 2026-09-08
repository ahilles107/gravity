//! Runtime adapter contract. No other daemon component may depend on Claude
//! Code process details; everything goes through these traits.
//!
//! Bus deliveries never go through the PTY: they are posted to the session's
//! inbox socket (see `channel`). The PTY carries only terminal I/O.

pub mod double;
pub mod pty;

use std::path::PathBuf;

use serde::Serialize;
use tokio::sync::mpsc;

use crate::channel::MsgSocket;

#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    pub kind: &'static str,
    /// The runtime supervises detached sessions itself (native background sessions).
    pub native_background: bool,
    /// Deliveries are injected through the session's inbox socket.
    pub channel_delivery: bool,
    /// Approval decisions can be relayed safely out-of-band.
    pub permission_relay: bool,
}

/// Everything an adapter needs to start one bot session.
#[derive(Debug, Clone)]
pub struct BotSpec {
    pub bot_id: String,
    pub bot_name: String,
    pub workspace: PathBuf,
    pub claude_bin: String,
    pub claude_args: Vec<String>,
    /// Environment injected into the session (bus token, MCP config, hooks).
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug)]
pub enum SessionEvent {
    Output(Vec<u8>),
    Exited { code: Option<i32> },
}

/// A freshly started runtime session.
pub struct StartedSession {
    pub session: Box<dyn RuntimeSession>,
    pub events: mpsc::UnboundedReceiver<SessionEvent>,
    /// Inbox socket when the adapter knows it at start. The PTY adapter
    /// learns it later from the SessionStart hook instead.
    pub msg_socket: Option<MsgSocket>,
}

/// A live runtime session for one bot. Terminal I/O only — deliveries go
/// through the inbox socket, never through here.
pub trait RuntimeSession: Send {
    /// Raw user keystrokes forwarded from the attached terminal.
    fn send_input(&mut self, bytes: &[u8]) -> anyhow::Result<()>;
    fn resize(&mut self, cols: u16, rows: u16) -> anyhow::Result<()>;
    fn kill(&mut self) -> anyhow::Result<()>;
}

pub trait RuntimeAdapter: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    /// Start a session. Output and exit events arrive on the returned channel.
    fn start(&self, spec: &BotSpec) -> anyhow::Result<StartedSession>;
    /// Cheap availability probe (binary present, auth intact where knowable).
    fn probe(&self) -> anyhow::Result<String>;
}
