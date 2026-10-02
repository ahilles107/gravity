use super::{approvals::Prompt, observations, transcript_path, BotSpec, SessionEvent, Wire};
use anyhow::Context;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Child;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) struct Worker {
    pub spec: BotSpec,
    pub input: super::transport::RpcWriter,
    pub native: bool,
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
    /// When the running turn started, for its recorded duration.
    pub turn_started: Option<Instant>,
    /// Paths a file change item touches, by item id, for its approval card.
    pub file_changes: HashMap<String, Vec<String>>,
}

pub(super) fn run(
    spec: BotSpec,
    input: super::transport::RpcWriter,
    native: bool,
    rx: mpsc::Receiver<Wire>,
    events: tokio::sync::mpsc::UnboundedSender<SessionEvent>,
    ready: mpsc::SyncSender<anyhow::Result<()>>,
    child: Arc<Mutex<Child>>,
) {
    let transcript = transcript_path(&spec.workspace);
    let mut worker = Worker {
        spec,
        input,
        native,
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
        turn_started: None,
        file_changes: HashMap::new(),
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
        if self.native {
            return;
        }
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
        self.input.write(value)
    }
    pub fn rpc(&mut self, method: &str, params: Value) -> anyhow::Result<Value> {
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
        self.rpc("initialize", json!({ "clientInfo": { "name": "gravity", "title": "Gravity", "version": env!("CARGO_PKG_VERSION") }, "capabilities": { "experimentalApi": true } }))?;
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
            if self.native {
                params["historyMode"] = json!("legacy");
            }
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
                if self.native {
                    params["historyMode"] = json!("legacy");
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
        if self.native {
            // Naming materializes an empty thread without a model turn.
            self.rpc(
                "thread/name/set",
                json!({ "threadId": self.thread, "name": self.spec.bot_name }),
            )?;
        }
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
                Wire::Answer(number, answer) => self.answer_card(number, answer),
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
    pub fn deliver(&mut self, text: &str) -> anyhow::Result<()> {
        if !self.native {
            observations::append(&self.transcript, "user", text)?;
        }
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
        if self.native
            && params
                .get("threadId")
                .and_then(Value::as_str)
                .is_some_and(|id| !self.thread.is_empty() && id != self.thread)
        {
            return Ok(());
        }
        match method {
            "turn/started" => {
                self.turn_started = Some(Instant::now());
                self.turn = params
                    .pointer("/turn/id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                self.hook("UserPromptSubmit", None);
            }
            "turn/completed" => {
                self.completed_turns += 1;
                self.turn = None;
                let duration = self.turn_started.take().map(|at| at.elapsed().as_millis());
                observations::turn_end(&self.transcript, duration)?;
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
                self.remember_file_change(params);
                if let Some(command) = params.pointer("/item/command").and_then(Value::as_str) {
                    self.output(&format!("\n$ {command}\n"));
                }
            }
            "item/completed" => {
                if self.native
                    && params.pointer("/item/type").and_then(Value::as_str) == Some("userMessage")
                {
                    if let Some(content) = params.pointer("/item/content").and_then(Value::as_array)
                    {
                        for item in content {
                            if let Some(text) = item.get("text").and_then(Value::as_str) {
                                observations::append(&self.transcript, "user", text)?;
                            }
                        }
                    }
                }
                if params.pointer("/item/type").and_then(Value::as_str) == Some("agentMessage") {
                    if let Some(text) = params.pointer("/item/text").and_then(Value::as_str) {
                        observations::append(&self.transcript, "assistant", text)?;
                    }
                }
                if let Some(item) = params.get("item") {
                    observations::tool_item(&self.transcript, item)?;
                    if let Some(id) = item.get("id").and_then(Value::as_str) {
                        self.file_changes.remove(id);
                    }
                }
                self.hook("PostToolUse", None);
            }
            "serverRequest/resolved" => self.resolved(params.get("requestId")),
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

impl Worker {
    /// Keeps a file change's paths until it completes: its approval request
    /// names only the item.
    fn remember_file_change(&mut self, params: &Value) {
        let Some(item) = params.get("item").filter(|i| i["type"] == "fileChange") else {
            return;
        };
        let paths = item["changes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|change| change["path"].as_str().map(str::to_string))
            .collect();
        if let Some(id) = item["id"].as_str() {
            self.file_changes.insert(id.to_string(), paths);
        }
    }
}
