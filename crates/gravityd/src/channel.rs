//! Channel delivery: post bus envelopes into a session's inbox socket
//! (Claude Code cross-session messaging). Newline-delimited JSON: an optional
//! auth line, then a user message. Verified against Claude Code 2.1.251.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;

/// Address of one session's inbox socket, reported by the SessionStart hook
/// (or provided directly by the runtime double).
#[derive(Debug, Clone)]
pub struct MsgSocket {
    pub path: PathBuf,
    /// `CLAUDE_CODE_MESSAGING_TOKEN`; optional on macOS but sent when known.
    pub token: Option<String>,
}

/// Deliver `text` as a user message. The receiving Claude reads it between
/// tool calls during an active turn, or it starts a new turn when idle —
/// never interleaving with terminal input.
pub fn send(socket: &MsgSocket, text: &str) -> anyhow::Result<()> {
    let mut stream = UnixStream::connect(&socket.path)
        .with_context(|| format!("connecting inbox socket {}", socket.path.display()))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let mut payload = String::new();
    if let Some(token) = &socket.token {
        payload.push_str(&serde_json::json!({ "type": "auth", "token": token }).to_string());
        payload.push('\n');
    }
    payload.push_str(
        &serde_json::json!({
            "type": "user",
            "message": { "role": "user", "content": text }
        })
        .to_string(),
    );
    payload.push('\n');

    stream
        .write_all(payload.as_bytes())
        .context("writing to inbox socket")?;
    stream.flush()?;
    Ok(())
}

pub fn socket_exists(path: &Path) -> bool {
    path.exists()
}
