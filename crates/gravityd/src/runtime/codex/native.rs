//! The official Codex terminal UI and Gravity share one private App Server.
use super::{start_server, transport::Remote, CodexAdapter};
use crate::runtime::{
    pty::PtyAdapter, BotSpec, Capabilities, RuntimeAdapter, RuntimeSession, SessionEvent,
    StartedSession,
};
use anyhow::Context;
use std::sync::{Arc, Mutex};

pub struct NativeCodexAdapter;

impl RuntimeAdapter for NativeCodexAdapter {
    fn capabilities(&self) -> Capabilities {
        CodexAdapter.capabilities()
    }
    fn probe(&self) -> anyhow::Result<String> {
        CodexAdapter.probe()
    }
    fn start(&self, spec: &BotSpec) -> anyhow::Result<StartedSession> {
        let remote = Remote::new()?;
        let mut backend = start_server(spec, Some(&remote))?;
        let codex = spec.codex.as_ref().context("missing Codex settings")?;
        let thread = std::fs::read_to_string(
            spec.workspace
                .parent()
                .unwrap_or(&spec.workspace)
                .join(super::THREAD_FILE),
        )?;
        let mut terminal = spec.clone();
        terminal.claude_bin = super::super::executable::command(&codex.bin)
            .get_program()
            .to_str()
            .context("Codex executable is not UTF-8")?
            .to_string();
        terminal.claude_args = codex.args.clone();
        terminal.claude_args.extend([
            "resume".to_string(),
            "--remote".to_string(),
            remote.address,
            "--remote-auth-token-env".to_string(),
            "GRAVITY_CODEX_REMOTE_TOKEN".to_string(),
            "--no-alt-screen".to_string(),
            thread.trim().to_string(),
        ]);
        terminal
            .env
            .push(("GRAVITY_CODEX_REMOTE_TOKEN".to_string(), remote.token));
        let ui = match PtyAdapter.start(&terminal) {
            Ok(ui) => ui,
            Err(error) => {
                let _ = backend.session.kill();
                return Err(error).context("start native Codex terminal");
            }
        };
        let shared = Arc::new(Mutex::new(Sessions {
            ui: ui.session,
            backend: backend.session,
            stopped: false,
            exit_reported: false,
        }));
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        for mut events in [ui.events, backend.events] {
            let sessions = shared.clone();
            let out = tx.clone();
            std::thread::spawn(move || {
                while let Some(event) = events.blocking_recv() {
                    if let SessionEvent::Exited { .. } = event {
                        let mut sessions = sessions.lock().unwrap_or_else(|e| e.into_inner());
                        if !sessions.exit_reported {
                            sessions.exit_reported = true;
                            let _ = sessions.kill();
                            let _ = out.send(event);
                        }
                        break;
                    }
                    if out.send(event).is_err() {
                        break;
                    }
                }
            });
        }
        Ok(StartedSession {
            session: Box::new(NativeSession(shared)),
            events: rx,
            msg_socket: None,
        })
    }
}

struct Sessions {
    ui: Box<dyn RuntimeSession>,
    backend: Box<dyn RuntimeSession>,
    stopped: bool,
    exit_reported: bool,
}

impl Sessions {
    fn kill(&mut self) -> anyhow::Result<()> {
        if self.stopped {
            return Ok(());
        }
        self.stopped = true;
        let ui = self.ui.kill();
        let backend = self.backend.kill();
        ui.and(backend)
    }
}

struct NativeSession(Arc<Mutex<Sessions>>);
impl RuntimeSession for NativeSession {
    fn send_input(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ui
            .send_input(bytes)
    }
    fn resize(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ui
            .resize(cols, rows)
    }
    fn deliver(&mut self, text: &str) -> Option<anyhow::Result<()>> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .backend
            .deliver(text)
    }
    fn answer_permission(
        &mut self,
        key: u64,
        answer: crate::runtime::PermissionAnswer,
    ) -> anyhow::Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .backend
            .answer_permission(key, answer)
    }
    fn kill(&mut self) -> anyhow::Result<()> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).kill()
    }
}
impl Drop for NativeSession {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}
