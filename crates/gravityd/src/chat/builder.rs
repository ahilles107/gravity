//! Folding transcript records into turns. Lines arrive in file order, once
//! each, so a live session is read incrementally: a new line extends the open
//! turn or starts the next one. Heavy content (tool input and output, image
//! bytes) is not kept; the builder records where it lives in the file.

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::envelope::{self, Delivered};
use super::model::{AsideKind, ChatItem, ChatTurn, ImageRef, Step, StepStatus, Trigger};
use super::steps::{self, patch_counts, result_text, BusCall, ToolKind};
use super::triggers::{self, MAX_TRIGGER_CHARS};

/// Longest text kept in a turn. A bot that pastes a whole file into its
/// answer is still readable, and the rest is in the terminal.
const MAX_TEXT_CHARS: usize = 20_000;

/// Where a step's heavy content lives in the transcript.
#[derive(Debug, Clone)]
pub struct StepLocator {
    pub use_offset: u64,
    pub result_offset: Option<u64>,
}

/// Where an image's bytes live: the line, the tool_result block, and the
/// image block inside it.
#[derive(Debug, Clone, Copy)]
pub struct ImageLocator {
    pub offset: u64,
    pub block: usize,
    pub inner: usize,
}

pub struct Builder {
    bot_id: String,
    /// Upper-case envelope names to the bots' real names.
    names: HashMap<String, String>,
    pub turns: Vec<ChatTurn>,
    /// tool_use id → (turn, item).
    calls: HashMap<String, (usize, usize)>,
    pub steps: HashMap<String, StepLocator>,
    pub images: HashMap<String, ImageLocator>,
    changed: BTreeSet<usize>,
    last_at: DateTime<Utc>,
}

impl Builder {
    pub fn new(bot_id: &str, names: HashMap<String, String>) -> Self {
        Self {
            bot_id: bot_id.to_string(),
            names,
            turns: Vec::new(),
            calls: HashMap::new(),
            steps: HashMap::new(),
            images: HashMap::new(),
            changed: BTreeSet::new(),
            last_at: DateTime::<Utc>::MIN_UTC,
        }
    }

    /// The turns that changed since the last call.
    pub fn take_changed(&mut self) -> Vec<ChatTurn> {
        std::mem::take(&mut self.changed)
            .into_iter()
            .filter_map(|i| self.turns.get(i).cloned())
            .collect()
    }

    pub fn push_line(&mut self, offset: u64, line: &str) {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            return;
        };
        if record.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            return;
        }
        let at = record
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or(self.last_at);
        self.last_at = at;
        let id = record
            .get("uuid")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("o{offset}"));
        match record.get("type").and_then(Value::as_str) {
            Some("user") => self.user(offset, &id, at, &record),
            Some("assistant") => self.assistant(offset, &id, at, &record),
            Some("system") => self.system(&id, at, &record),
            Some("attachment") => self.attachment(&id, at, &record),
            _ => {}
        }
    }

    fn user(&mut self, offset: u64, id: &str, at: DateTime<Utc>, record: &Value) {
        let content = &record["message"]["content"];
        if let Some(text) = content.as_str() {
            self.prompt(id, at, record, text);
            return;
        }
        let Some(blocks) = content.as_array() else {
            return;
        };
        let mut text = String::new();
        for (index, block) in blocks.iter().enumerate() {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_result") => self.result(offset, index, block, record),
                Some("text") => text.push_str(block["text"].as_str().unwrap_or_default()),
                _ => {}
            }
        }
        if text.starts_with("[Request interrupted") {
            let turn = self.current(at);
            self.aside(turn, id, AsideKind::Interrupted, "Interrupted".to_string());
        } else if !text.is_empty() && !blocks.iter().any(|b| b["type"] == "tool_result") {
            // A prompt written as text blocks: Codex observations, or input
            // with pasted images.
            self.prompt(id, at, record, &text);
        }
    }

    /// A user record that starts a turn, unless it is runtime bookkeeping.
    fn prompt(&mut self, id: &str, at: DateTime<Utc>, record: &Value, text: &str) {
        if let Some(trigger) = triggers::of_prompt(record, text, &self.names) {
            self.start(id, at, trigger);
        }
    }

    fn assistant(&mut self, offset: u64, id: &str, at: DateTime<Utc>, record: &Value) {
        let Some(blocks) = record["message"]["content"].as_array() else {
            return;
        };
        let turn = self.current(at);
        for (index, block) in blocks.iter().enumerate() {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let text = block["text"].as_str().unwrap_or_default().trim();
                    if !text.is_empty() {
                        self.text(turn, &format!("{id}:{index}"), text);
                    }
                }
                Some("tool_use") => self.call(turn, offset, block),
                _ => {}
            }
        }
    }

    fn text(&mut self, turn: usize, id: &str, text: &str) {
        let items = &mut self.turns[turn].items;
        if let Some(ChatItem::Text { markdown, .. }) = items.last_mut() {
            markdown.push_str("\n\n");
            markdown.push_str(text);
            *markdown = steps::truncate(markdown, MAX_TEXT_CHARS);
        } else {
            items.push(ChatItem::Text {
                id: id.to_string(),
                markdown: steps::truncate(text, MAX_TEXT_CHARS),
            });
        }
        self.changed.insert(turn);
    }

    fn call(&mut self, turn: usize, offset: u64, block: &Value) {
        let id = block["id"].as_str().unwrap_or_default().to_string();
        let name = block["name"].as_str().unwrap_or_default();
        let input = &block["input"];
        let stats = &mut self.turns[turn].stats;
        let item = match steps::bus_call(name, input) {
            Some(BusCall::Sent { to, kind, body }) => {
                stats.sent += 1;
                ChatItem::Sent {
                    id: id.clone(),
                    to,
                    msg_kind: kind,
                    body: steps::truncate(&body, MAX_TRIGGER_CHARS),
                }
            }
            Some(BusCall::Completed {
                task_id,
                result,
                artifacts,
            }) => ChatItem::Completed {
                id: id.clone(),
                task_id,
                result: steps::truncate(&result, MAX_TEXT_CHARS),
                artifacts,
            },
            Some(BusCall::Decision { title }) => ChatItem::Decision {
                id: id.clone(),
                decision_id: None,
                title,
            },
            None => {
                let described = steps::describe(name, input);
                match described.kind {
                    ToolKind::Command => stats.commands += 1,
                    ToolKind::Read => stats.reads += 1,
                    ToolKind::Edit => stats.edits += 1,
                    ToolKind::Other => {}
                }
                ChatItem::Step(Step {
                    id: id.clone(),
                    tool: name.to_string(),
                    title: described.title,
                    subtitle: described.subtitle,
                    status: StepStatus::Running,
                    minor: described.minor,
                    added: None,
                    removed: None,
                    images: Vec::new(),
                })
            }
        };
        let items = &mut self.turns[turn].items;
        self.calls.insert(id.clone(), (turn, items.len()));
        items.push(item);
        self.steps.insert(
            id,
            StepLocator {
                use_offset: offset,
                result_offset: None,
            },
        );
        self.changed.insert(turn);
    }

    fn result(&mut self, offset: u64, index: usize, block: &Value, record: &Value) {
        let use_id = block["tool_use_id"].as_str().unwrap_or_default();
        let Some(&(turn, item)) = self.calls.get(use_id) else {
            return;
        };
        if let Some(locator) = self.steps.get_mut(use_id) {
            locator.result_offset = Some(offset);
        }
        let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
        let mut images = Vec::new();
        for (inner, part) in block["content"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            if part["type"] == "image" {
                let image_id = format!("{offset}:{index}:{inner}");
                let mime = part["source"]["media_type"].as_str().unwrap_or("image/png");
                self.images.insert(
                    image_id.clone(),
                    ImageLocator {
                        offset,
                        block: index,
                        inner,
                    },
                );
                images.push(ImageRef {
                    id: image_id,
                    mime: mime.to_string(),
                });
            }
        }
        let (added, removed) = patch_counts(&record["toolUseResult"]);
        let turn_ref = &mut self.turns[turn];
        turn_ref.stats.images += images.len() as u32;
        turn_ref.stats.added += added;
        turn_ref.stats.removed += removed;
        if failed {
            turn_ref.stats.errors += 1;
        }
        match &mut turn_ref.items[item] {
            ChatItem::Step(step) => {
                step.status = if failed {
                    StepStatus::Error
                } else {
                    StepStatus::Ok
                };
                step.images.extend(images);
                if added + removed > 0 {
                    step.added = Some(added);
                    step.removed = Some(removed);
                }
            }
            ChatItem::Decision { decision_id, .. } => {
                *decision_id = result_text(block)
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                    .and_then(|v| v["id"].as_str().map(str::to_string));
            }
            _ => {}
        }
        self.changed.insert(turn);
    }

    fn system(&mut self, id: &str, at: DateTime<Utc>, record: &Value) {
        match record.get("subtype").and_then(Value::as_str) {
            Some("turn_duration") => {
                if let Some(turn) = self.turns.last_mut().filter(|t| t.open) {
                    turn.open = false;
                    turn.ended_at = Some(at);
                    turn.duration_ms = record.get("durationMs").and_then(Value::as_i64);
                    self.changed.insert(self.turns.len() - 1);
                }
            }
            Some("compact_boundary") => {
                let turn = self.current(at);
                self.aside(
                    turn,
                    id,
                    AsideKind::Compacted,
                    "Conversation compacted".to_string(),
                );
            }
            _ => {}
        }
    }

    fn attachment(&mut self, id: &str, at: DateTime<Utc>, record: &Value) {
        let attachment = &record["attachment"];
        if attachment["type"] != "queued_command" {
            return;
        }
        let prompt = attachment["prompt"].as_str().unwrap_or_default();
        let text = match envelope::parse(prompt) {
            Some(Delivered::Message {
                from, kind, body, ..
            }) => {
                let from = self.names.get(&from).cloned().unwrap_or(from);
                format!("From {from} · {kind}: {}", steps::truncate(&body, 600))
            }
            _ => steps::truncate(prompt, 600),
        };
        let turn = self.current(at);
        self.aside(turn, id, AsideKind::Incoming, text);
    }

    fn aside(&mut self, turn: usize, id: &str, kind: AsideKind, text: String) {
        self.turns[turn].items.push(ChatItem::Aside {
            id: id.to_string(),
            kind,
            text,
        });
        self.changed.insert(turn);
    }

    fn start(&mut self, id: &str, at: DateTime<Utc>, trigger: Trigger) {
        if let Some(previous) = self.turns.last_mut().filter(|t| t.open) {
            previous.open = false;
            self.changed.insert(self.turns.len() - 1);
        }
        self.turns.push(ChatTurn {
            id: id.to_string(),
            bot_id: self.bot_id.clone(),
            started_at: at,
            ended_at: None,
            duration_ms: None,
            open: true,
            trigger,
            items: Vec::new(),
            stats: Default::default(),
        });
        self.changed.insert(self.turns.len() - 1);
    }

    /// The turn new content belongs to: the open one, or a resumed turn when
    /// the transcript picks up mid-conversation.
    fn current(&mut self, at: DateTime<Utc>) -> usize {
        if !self.turns.last().is_some_and(|t| t.open) {
            let id = format!("resumed-{}", at.timestamp_millis());
            self.start(&id, at, Trigger::Resumed);
        }
        self.turns.len() - 1
    }
}
