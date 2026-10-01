//! Codex CLI App Server over stdio JSONL. Bus input is a separate RPC stream.
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use super::{BotSpec, Capabilities, RuntimeAdapter, RuntimeSession, SessionEvent, StartedSession};
use anyhow::Context;
use serde_json::Value;

mod approvals;
mod observations;
mod worker;

#[derive(Debug, Clone)]
pub struct CodexSpec {
    pub bin: String,
    pub args: Vec<String>,
    pub port: u16,
    pub artifacts: Option<PathBuf>,
}

pub struct CodexAdapter;

pub(super) enum Wire {
    Server(Value),
    Closed,
    Input(Vec<u8>),
    Deliver(String, mpsc::SyncSender<anyhow::Result<()>>),
    Stop,
}

pub fn transcript_path(workspace: &Path) -> PathBuf {
    workspace
        .parent()
        .unwrap_or(workspace)
        .join("codex-observations.jsonl")
}

impl RuntimeAdapter for CodexAdapter {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            kind: "codex_cli",
            native_background: false,
            channel_delivery: true,
            permission_relay: true,
        }
    }
    fn probe(&self) -> anyhow::Result<String> {
        let out = Command::new("codex").arg("--version").output()?;
        anyhow::ensure!(out.status.success(), "codex --version failed");
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
    fn start(&self, spec: &BotSpec) -> anyhow::Result<StartedSession> {
        let codex = spec
            .codex
            .as_ref()
            .context("missing Codex runtime settings")?;
        let mut command = Command::new(&codex.bin);
        command
            .args(&codex.args)
            .arg("app-server")
            .args(["--listen", "stdio://"])
            .current_dir(&spec.workspace)
            .envs(spec.env.iter().cloned())
            .env_remove("CODEX_THREAD_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().context("spawn Codex CLI App Server")?;
        let stdin = child.stdin.take().context("Codex stdin")?;
        let stdout = child.stdout.take().context("Codex stdout")?;
        let stderr = child.stderr.take().context("Codex stderr")?;
        let child = Arc::new(Mutex::new(child));
        let (tx, rx) = mpsc::channel();
        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let reader_tx = tx.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => match serde_json::from_str(&line) {
                        Ok(value) => {
                            if reader_tx.send(Wire::Server(value)).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    },
                }
            }
            let _ = reader_tx.send(Wire::Closed);
        });
        let err_tx = events_tx.clone();
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        let captured = stderr_tail.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut tail = captured.lock().unwrap_or_else(|e| e.into_inner());
                if tail.len() + line.len() > 32_768 {
                    tail.clear();
                }
                tail.push_str(&line);
                tail.push('\n');
                drop(tail);
                if err_tx
                    .send(SessionEvent::Output(
                        format!("[codex] {line}\r\n").into_bytes(),
                    ))
                    .is_err()
                {
                    break;
                }
            }
        });
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let spec = spec.clone();
        let worker_child = child.clone();
        std::thread::spawn(move || worker::run(spec, stdin, rx, events_tx, ready_tx, worker_child));
        let ready = ready_rx
            .recv_timeout(Duration::from_secs(30))
            .context("Codex initialization timed out");
        match ready {
            Ok(Ok(())) => Ok(StartedSession {
                session: Box::new(CodexSession { tx, child }),
                events: events_rx,
                msg_socket: None,
            }),
            outcome => {
                let _ = child.lock().unwrap_or_else(|e| e.into_inner()).kill();
                let detail = stderr_tail
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                match outcome {
                    Ok(Err(error)) | Err(error) => {
                        if detail.trim().is_empty() {
                            Err(error)
                        } else {
                            Err(error).context(detail.trim().to_string())
                        }
                    }
                    Ok(Ok(())) => unreachable!(),
                }
            }
        }
    }
}

struct CodexSession {
    tx: mpsc::Sender<Wire>,
    child: Arc<Mutex<Child>>,
}

impl RuntimeSession for CodexSession {
    fn send_input(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.tx
            .send(Wire::Input(bytes.to_vec()))
            .map_err(|_| anyhow::anyhow!("Codex session stopped"))
    }
    fn deliver(&mut self, text: &str) -> Option<anyhow::Result<()>> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        Some((|| {
            self.tx
                .send(Wire::Deliver(text.to_string(), reply_tx))
                .map_err(|_| anyhow::anyhow!("Codex session stopped"))?;
            reply_rx
                .recv_timeout(Duration::from_secs(12))
                .context("Codex delivery timed out")?
        })())
    }
    fn resize(&mut self, _cols: u16, _rows: u16) -> anyhow::Result<()> {
        Ok(())
    }
    fn kill(&mut self) -> anyhow::Result<()> {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if child.try_wait()?.is_none() {
            child.kill()?;
        }
        let _ = self.tx.send(Wire::Stop);
        Ok(())
    }
}

impl Drop for CodexSession {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}
