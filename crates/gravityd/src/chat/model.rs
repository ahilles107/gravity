//! The chat wire model: a bot's conversation as turns. See
//! `docs/superpowers/specs/2026-09-16-chat-pane-design.md`.

use chrono::{DateTime, Utc};
use serde::Serialize;

/// One turn: what woke the bot, and everything it did until it stopped.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatTurn {
    /// The transcript id of the record that opened the turn.
    pub id: String,
    pub bot_id: String,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    pub open: bool,
    pub trigger: Trigger,
    pub items: Vec<ChatItem>,
    pub stats: Stats,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    /// The owner: from the app's composer, or typed into the terminal.
    Owner { text: String, via: OwnerVia },
    /// Another bot's message on the bus.
    Bus {
        from: String,
        msg_kind: String,
        num: i64,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
    },
    Routine {
        name: String,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        run_id: Option<String>,
    },
    /// A decision ruling or notice delivered by the registry.
    Ruling { decision_id: String, text: String },
    /// The session carried on after its context was compacted.
    Resumed,
    /// A background task finishing, a scheduled wake-up, anything else the
    /// runtime started a turn for.
    Background { text: String },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OwnerVia {
    Chat,
    Terminal,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatItem {
    Text {
        id: String,
        markdown: String,
    },
    Step(Step),
    /// `send_message` on the bus.
    Sent {
        id: String,
        to: String,
        msg_kind: String,
        body: String,
    },
    /// `complete_task`: the result and the files it handed back.
    Completed {
        id: String,
        task_id: String,
        result: String,
        artifacts: Vec<FileRef>,
    },
    /// `raise_decision`: the bot asked the owner to decide.
    Decision {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        decision_id: Option<String>,
        title: String,
    },
    Aside {
        id: String,
        kind: AsideKind,
        text: String,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Step {
    /// The tool call's id.
    pub id: String,
    pub tool: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub status: StepStatus,
    /// Housekeeping (inbox checks, tool lookups) that folds away.
    pub minor: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageRef>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Running,
    Ok,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AsideKind {
    /// The context was compacted.
    Compacted,
    /// The owner interrupted the turn.
    Interrupted,
    /// A message arrived while the bot was working.
    Incoming,
}

/// An image a step produced or looked at, fetched with `get_chat_image`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImageRef {
    pub id: String,
    pub mime: String,
}

/// A file a result names, opened with `read_file`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileRef {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Stats {
    pub commands: u32,
    pub reads: u32,
    pub edits: u32,
    pub added: u32,
    pub removed: u32,
    pub sent: u32,
    pub images: u32,
    pub errors: u32,
}

/// The heavy half of a step, loaded when the owner opens it.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct StepDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}
