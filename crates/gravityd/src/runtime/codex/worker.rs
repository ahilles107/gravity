use super::{approvals::Prompt, observations, transcript_path, BotSpec, SessionEvent, Wire};
use anyhow::Context;
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) struct Worker {
    pub spec: BotSpec,
    pub input: BufWriter<ChildStdin>,
    pub rx: mpsc::Receiver<Wire>,
    pub events: tokio::sync::mpsc::UnboundedSender<SessionEvent>,
    pub queued: VecDeque<Wire>,
    pub serial: u64,
    pub thread: String,
    pub turn: Option<String>,
    pub line: Vec<u8>,
    pub escaping: bool,
    pub prompts: BTreeMap<u64, Prompt>,
    pub prompt_serial: u64,
    pub transcript: PathBuf,
    pub dead: bool,
    pub completed_turns: u64,
}

pub(super) fn run(
    spec: BotSpec,
    stdin: ChildStdin,
    rx: mpsc::Receiver<Wire>,
    events: tokio::sync::mpsc::UnboundedSender<SessionEvent>,
    ready: mpsc::SyncSender<anyhow::Result<()>>,
    child: Arc<Mutex<Child>>,
) {
    let transcript = transcript_path(&spec.workspace);
    let mut worker = Worker {
        spec,
        input: BufWriter::new(stdin),
        rx,
        events,
        queued: VecDeque::new(),
        serial: 0,
        thread: String::new(),
        turn: None,
        line: Vec::new(),
        escaping: false,
        prompts: BTreeMap::new(),
        prompt_serial: 0,
        transcript,
        dead: false,
        completed_turns: 0,
    };
    let result = worker.initialize();
    match result {
        Ok(()) => {
            if ready.send(Ok(())).is_ok() {
                worker.main_loop();
            }
        }
        Err(error) => {
            let _ = ready.send(Err(error));
        }
    }
    let code = {
        let mut child = child.lock().unwrap_or_else(|e| e.into_inner());
        let _ = child.kill();
        child.wait().ok().and_then(|status| status.code())
    };
    let _ = worker.events.send(SessionEvent::Exited { code });
}

impl Worker {
    pub fn output(&self, text: &str) {
        let _ = self.events.send(SessionEvent::Output(
            text.replace('\n', "\r\n").into_bytes(),
        ));
    }
    pub fn hook(&self, event: &'static str, detail: Option<String>) {
        let transcript = (event == "Stop").then(|| self.transcript.display().to_string());
        let _ = self.events.send(SessionEvent::Lifecycle {
            event,
            detail,
            transcript,
        });
    }
    pub fn write(&mut self, value: &Value) -> anyhow::Result<()> {
        serde_json::to_writer(&mut self.input, value)?;
        self.input.write_all(b"\n")?;
        self.input.flush()?;
        Ok(())
    }
    fn rpc(&mut self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.serial += 1;
        let id = self.serial;
        self.write(&json!({ "id": id, "method": method, "params": params }))?;
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let message = match self
                .rx
                .recv_timeout(until.saturating_duration_since(Instant::now()))
            {
                Ok(message) => message,
                Err(error) => {
                    self.dead = true;
                    return Err(error)
                        .with_context(|| format!("Codex {method} response timed out"));
                }
            };
            match message {
                Wire::Server(value)
                    if value.get("id") == Some(&json!(id)) && value.get("method").is_none() =>
                {
                    if let Some(error) = value.get("error") {
                        anyhow::bail!(
                            "Codex {method}: {}",
                            error
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("request failed")
                        );
                    }
                    return value
                        .get("result")
                        .cloned()
                        .context("missing Codex RPC result");
                }
                Wire::Server(value) => self.server(value)?,
                Wire::Closed | Wire::Stop => {
                    self.dead = true;
                    anyhow::bail!("Codex process stopped");
                }
                command => self.queued.push_back(command),
            }
        }
    }
    fn initialize(&mut self) -> anyhow::Result<()> {
        self.rpc("initialize", json!({ "clientInfo": { "name": "gravity", "title": "Gravity", "version": env!("CARGO_PKG_VERSION") } }))?;
        self.write(&json!({ "method": "initialized", "params": {} }))?;
        let codex = self.spec.codex.as_ref().context("missing Codex settings")?;
        let mut roots = vec![self.spec.workspace.display().to_string()];
        if let Some(artifacts) = &codex.artifacts {
            roots.push(artifacts.display().to_string());
        }
        let mut params = json!({
            "cwd": self.spec.workspace,
            "approvalPolicy": "on-request", "sandbox": "workspace-write",
            "developerInstructions": self.spec.workspace.parent().and_then(|root| std::fs::read_to_string(root.join("system.md")).ok()).unwrap_or_default()
                + "\nRead CLAUDE.md and FACTS.md for your saved context. Keep durable facts in FACTS.md.",
            "config": {
                "mcp_servers.gravity-bus": { "url": format!("http://127.0.0.1:{}/mcp", codex.port), "bearer_token_env_var": "GRAVITY_TOKEN" },
                "sandbox_workspace_write.writable_roots": roots
            }
        });
        let path = self
            .spec
            .workspace
            .parent()
            .unwrap_or(&self.spec.workspace)
            .join("codex-thread-id");
        let saved = std::fs::read_to_string(&path)
            .ok()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty());
        let method = if let Some(id) = saved {
            params["threadId"] = json!(id);
            "thread/resume"
        } else {
            "thread/start"
        };
        let reply = match self.rpc(method, params.clone()) {
            Err(error)
                if method == "thread/resume"
                    && error.to_string().contains("no rollout found for thread id")
                    && std::fs::metadata(&self.transcript).map_or(true, |m| m.len() == 0) =>
            {
                // App Server creates a rollout only after the first turn.
                if let Some(object) = params.as_object_mut() {
                    object.remove("threadId");
                }
                self.rpc("thread/start", params)?
            }
            result => result?,
        };
        self.thread = reply
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .context("Codex did not return a thread ID")?
            .to_string();
        std::fs::write(path, format!("{}\n", self.thread))?;
        self.output("Codex CLI ready. Type a prompt and press Enter. /interrupt stops a turn.\n");
        self.hook("SessionStart", None);
        Ok(())
    }
    fn main_loop(&mut self) {
        while let Some(message) = self.queued.pop_front().or_else(|| self.rx.recv().ok()) {
            let result = match message {
                Wire::Server(value) => self.server(value),
                Wire::Input(bytes) => self.keystrokes(&bytes),
                Wire::Deliver(text, reply) => {
                    let result = self.deliver(&text);
                    let _ = reply.send(result);
                    Ok(())
                }
                Wire::Closed | Wire::Stop => break,
            };
            if let Err(error) = result {
                self.output(&format!("\n[Codex error] {error}\n"));
            }
            if self.dead {
                break;
            }
        }
    }
    fn deliver(&mut self, text: &str) -> anyhow::Result<()> {
        observations::append(&self.transcript, "user", text)?;
        let input = json!([{ "type": "text", "text": text }]);
        if let Some(turn) = &self.turn {
            match self.rpc(
                "turn/steer",
                json!({ "threadId": self.thread, "expectedTurnId": turn, "input": input }),
            ) {
                Ok(_) => return Ok(()),
                Err(error) if self.dead || self.turn.is_some() => return Err(error),
                Err(_) => {} // The completed notification raced the steering request.
            }
        }
        {
            let completed = self.completed_turns;
            let reply = self.rpc(
                "turn/start",
                json!({ "threadId": self.thread, "input": input }),
            )?;
            // A completed notification can precede the reply for a fast turn.
            if self.completed_turns == completed
                && reply.pointer("/turn/status").and_then(Value::as_str) == Some("inProgress")
            {
                self.turn = reply
                    .pointer("/turn/id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
        }
        Ok(())
    }
    fn keystrokes(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        for &byte in bytes {
            if self.escaping {
                if byte != b'[' && (0x40..=0x7e).contains(&byte) {
                    self.escaping = false;
                }
                continue;
            }
            match byte {
                27 => self.escaping = true,
                3 => {
                    self.line.clear();
                    self.interrupt()?;
                }
                8 | 127 => {
                    if let Some(last) = self.line.pop() {
                        if last & 0xc0 == 0x80 {
                            while self.line.pop().is_some_and(|b| b & 0xc0 == 0x80) {}
                        }
                        self.output("\u{8} \u{8}");
                    }
                }
                13 | 10 if !self.line.is_empty() => {
                    let text = String::from_utf8(std::mem::take(&mut self.line))
                        .context("prompt is not UTF-8")?;
                    self.output("\n");
                    if text == "/interrupt" {
                        self.interrupt()?;
                    } else if !self.answer(&text)? {
                        self.deliver(&text)?;
                    }
                }
                13 | 10 => {}
                b if b >= 32 => self.line.push(b),
                _ => {}
            }
        }
        // Echo whole UTF-8 inputs rather than one byte per output frame.
        if !bytes
            .iter()
            .any(|b| matches!(b, 3 | 8 | 10 | 13 | 27 | 127))
        {
            let _ = self.events.send(SessionEvent::Output(bytes.to_vec()));
        }
        Ok(())
    }
    fn interrupt(&mut self) -> anyhow::Result<()> {
        if let Some(turn) = &self.turn {
            self.rpc(
                "turn/interrupt",
                json!({ "threadId": self.thread, "turnId": turn }),
            )?;
        }
        Ok(())
    }
    fn server(&mut self, value: Value) -> anyhow::Result<()> {
        let Some(method) = value.get("method").and_then(Value::as_str) else {
            return Ok(());
        };
        if let Some(id) = value.get("id") {
            return self.prompt(
                id.clone(),
                method,
                value.get("params").cloned().unwrap_or(Value::Null),
            );
        }
        let params = value.get("params").unwrap_or(&Value::Null);
        match method {
            "turn/started" => {
                self.turn = params
                    .pointer("/turn/id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                self.hook("UserPromptSubmit", None);
            }
            "turn/completed" => {
                self.completed_turns += 1;
                self.turn = None;
                self.output("\n");
                if let Some(error) = params
                    .pointer("/turn/error/message")
                    .and_then(Value::as_str)
                {
                    self.output(error);
                }
                if params.pointer("/turn/status").and_then(Value::as_str) == Some("completed") {
                    self.hook("Stop", None);
                } else {
                    self.hook("TurnInterrupted", None);
                }
            }
            "item/agentMessage/delta" | "item/commandExecution/outputDelta" => {
                if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                    self.output(delta);
                }
            }
            "item/started" => {
                if let Some(command) = params.pointer("/item/command").and_then(Value::as_str) {
                    self.output(&format!("\n$ {command}\n"));
                }
            }
            "item/completed" => {
                if params.pointer("/item/type").and_then(Value::as_str) == Some("agentMessage") {
                    if let Some(text) = params.pointer("/item/text").and_then(Value::as_str) {
                        observations::append(&self.transcript, "assistant", text)?;
                    }
                }
                self.hook("PostToolUse", None);
            }
            "serverRequest/resolved" => {
                let request = params.get("requestId");
                self.prompts.retain(|_, prompt| Some(&prompt.id) != request);
            }
            "error" => {
                if let Some(message) = params.pointer("/error/message").and_then(Value::as_str) {
                    self.output(&format!("\n[Codex] {message}\n"));
                }
            }
            _ => {}
        }
        Ok(())
    }
}
