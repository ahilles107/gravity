//! Deterministic runtime double used by tests and by `runtime = "double"`
//! config. It binds a real inbox socket (same wire protocol as Claude Code
//! cross-session messaging) and echoes delivered messages and typed input to
//! its terminal output, so the full channel-delivery path is exercised.

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;

use super::{BotSpec, Capabilities, RuntimeAdapter, RuntimeSession, SessionEvent, StartedSession};
use crate::channel::MsgSocket;

/// Ctrl-D: ends the double session as an unsolicited exit.
const EOF: u8 = 0x04;

pub struct DoubleAdapter;

impl RuntimeAdapter for DoubleAdapter {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            kind: "double",
            native_background: false,
            channel_delivery: true,
            permission_relay: false,
        }
    }

    fn start(&self, spec: &BotSpec) -> anyhow::Result<StartedSession> {
        let (tx, rx) = mpsc::unbounded_channel();
        // The greeting repeats the command line it was spawned with, so tests
        // can see what the daemon asked for — `--continue` and the artifacts
        // grant above all. Everything but the system prompt, which is a whole
        // document and would bury the rest.
        let mut rendered: Vec<&str> = Vec::new();
        let mut skip_value = false;
        for arg in &spec.claude_args {
            if std::mem::take(&mut skip_value) {
                continue;
            }
            skip_value = arg == "--append-system-prompt";
            rendered.push(arg);
        }
        let flags = rendered.join(" ");
        let _ = tx.send(SessionEvent::Output(
            format!("double runtime ready for {} [{flags}]\r\n", spec.bot_name).into_bytes(),
        ));

        // Real inbox socket speaking the cross-session wire protocol. Unix
        // socket paths are capped at ~104 bytes on macOS, so keep it short:
        // /tmp + pid + a bot-id prefix.
        let sock_dir = std::path::PathBuf::from(format!("/tmp/cbd-{}", std::process::id()));
        std::fs::create_dir_all(&sock_dir)?;
        let short_id: String = spec.bot_id.chars().take(8).collect();
        let sock_path = sock_dir.join(format!("{short_id}.sock"));
        let _ = std::fs::remove_file(&sock_path);
        let listener = UnixListener::bind(&sock_path)?;
        let alive = Arc::new(AtomicBool::new(true));

        let inbox_tx = tx.clone();
        let inbox_alive = alive.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                if !inbox_alive.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(conn) = conn else {
                    break;
                };
                for line in BufReader::new(conn).lines() {
                    let Ok(line) = line else {
                        break;
                    };
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                        continue;
                    };
                    if v.get("type").and_then(|t| t.as_str()) == Some("user") {
                        let content = v
                            .pointer("/message/content")
                            .and_then(|c| c.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let _ = inbox_tx
                            .send(SessionEvent::Output(format!("{content}\r\n").into_bytes()));
                    }
                }
            }
        });

        Ok(StartedSession {
            session: Box::new(DoubleSession {
                tx,
                alive,
                sock_path: sock_path.clone(),
            }),
            events: rx,
            msg_socket: Some(MsgSocket {
                path: sock_path,
                token: None,
            }),
        })
    }

    fn probe(&self) -> anyhow::Result<String> {
        Ok("double 0.0.0".to_string())
    }
}

struct DoubleSession {
    tx: mpsc::UnboundedSender<SessionEvent>,
    alive: Arc<AtomicBool>,
    sock_path: std::path::PathBuf,
}

impl RuntimeSession for DoubleSession {
    fn send_input(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        if !self.alive.load(Ordering::SeqCst) {
            anyhow::bail!("session terminated");
        }
        // EOF ends the session the way it would end a real CLI. The daemon did
        // not ask for it, so this is the unsolicited-exit path — how tests
        // reach crash handling and the supervision restart.
        if bytes.contains(&EOF) {
            self.end(Some(0));
            return Ok(());
        }
        self.tx
            .send(SessionEvent::Output(bytes.to_vec()))
            .map_err(|_| anyhow::anyhow!("output channel closed"))
    }

    fn resize(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        if !self.alive.load(Ordering::SeqCst) {
            anyhow::bail!("session terminated");
        }
        // Surfaced as terminal output so tests can observe the sequence of
        // sizes a runtime would see — the double's stand-in for SIGWINCH.
        self.tx
            .send(SessionEvent::Output(
                format!("[resize {cols}x{rows}]").into_bytes(),
            ))
            .map_err(|_| anyhow::anyhow!("output channel closed"))
    }

    fn kill(&mut self) -> anyhow::Result<()> {
        self.end(Some(0));
        Ok(())
    }
}

impl DoubleSession {
    fn end(&mut self, code: Option<i32>) {
        self.alive.store(false, Ordering::SeqCst);
        let _ = std::fs::remove_file(&self.sock_path);
        let _ = self.tx.send(SessionEvent::Exited { code });
    }
}
