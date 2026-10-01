//! Channel delivery: post bus envelopes into a session's inbox socket
//! (Claude Code cross-session messaging). Newline-delimited JSON: an optional
//! auth line, then a user message. Verified against Claude Code 2.1.251.

#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
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
    #[cfg(unix)]
    let mut stream = UnixStream::connect(&socket.path)
        .with_context(|| format!("connecting inbox socket {}", socket.path.display()))?;
    #[cfg(unix)]
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

    #[cfg(unix)]
    {
        stream
            .write_all(payload.as_bytes())
            .context("writing to inbox socket")?;
        stream.flush()?;
    }
    #[cfg(windows)]
    send_pipe(socket, payload)?;
    Ok(())
}

pub fn socket_exists(path: &Path) -> bool {
    path.exists() || cfg!(windows) && path.to_string_lossy().starts_with(r"\\.\pipe\")
}

#[cfg(windows)]
fn send_pipe(socket: &MsgSocket, payload: String) -> anyhow::Result<()> {
    anyhow::ensure!(
        socket.path.to_string_lossy().starts_with(r"\\.\pipe\"),
        "inbox address must be a Windows named pipe"
    );
    anyhow::ensure!(
        socket.token.as_ref().is_some_and(|t| !t.is_empty()),
        "Windows inbox requires a messaging token"
    );
    let path = socket.path.clone();
    // A separate I/O runtime bounds writes without nesting Tokio block_on.
    std::thread::spawn(move || -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        use tokio::net::windows::named_pipe::ClientOptions;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), async {
                let mut pipe = loop {
                    match ClientOptions::new().open(&path) {
                        Ok(pipe) => break pipe,
                        Err(error) if error.raw_os_error() == Some(231) => {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                        Err(error) => return Err(error).context("connecting Windows inbox pipe"),
                    }
                };
                pipe.write_all(payload.as_bytes())
                    .await
                    .context("writing Windows inbox pipe")?;
                pipe.flush().await?;
                Ok(())
            })
            .await
            .context("Windows inbox delivery timed out")?
        })
    })
    .join()
    .map_err(|_| anyhow::anyhow!("Windows inbox worker panicked"))?
}
