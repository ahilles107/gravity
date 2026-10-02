//! Per-bot chat state kept in step with the transcript. A bot's chat is
//! built the first time a client asks for it, then extended line by line as
//! the transcript grows; a new transcript file (a fresh session) rebuilds it.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bus::{Bot, BotRuntime, BotState};
use serde_json::Value;

use crate::app::AppState;

use super::builder::Builder;
use super::model::{ChatTurn, StepDetail};
use super::steps::{self, result_text};

const MAX_INPUT_CHARS: usize = 4_000;
const MAX_OUTPUT_CHARS: usize = 12_000;
const MAX_CONTENT_CHARS: usize = 8_000;
const MAX_DIFF_LINES: usize = 400;

struct BotChat {
    path: Option<PathBuf>,
    /// Bytes of the transcript already folded in; always at a line boundary.
    offset: u64,
    builder: Builder,
}

#[derive(Default)]
pub struct ChatStore {
    bots: Mutex<HashMap<String, Arc<Mutex<BotChat>>>>,
}

impl ChatStore {
    /// Brings the bot's chat up to date and returns the turns that changed.
    pub fn refresh(&self, app: &AppState, bot: &Bot) -> anyhow::Result<Vec<ChatTurn>> {
        let (chat, fresh) = self.entry(app, bot)?;
        let mut chat = lock(&chat);
        let path = transcript(app, bot);
        if path != chat.path {
            *chat = BotChat {
                path: path.clone(),
                offset: 0,
                builder: Builder::new(&bot.id, names(app, bot)),
            };
        }
        if let Some(path) = &path {
            let offset = chat.offset;
            chat.offset = read_from(path, offset, &mut chat.builder)?;
        }
        let changed = chat.builder.take_changed();
        // The first read is a snapshot, not news.
        let changed = if fresh { Vec::new() } else { changed };
        Ok(settle(changed, &chat.builder.turns, busy(app, bot)))
    }

    /// Up to `limit` turns before `before` (or the newest), oldest first,
    /// and whether older ones exist.
    pub fn page(
        &self,
        app: &AppState,
        bot: &Bot,
        before: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<(Vec<ChatTurn>, bool)> {
        self.refresh(app, bot)?;
        let (chat, _) = self.entry(app, bot)?;
        let chat = lock(&chat);
        let turns = &chat.builder.turns;
        let end = match before {
            Some(id) => turns.iter().position(|t| t.id == id).unwrap_or(turns.len()),
            None => turns.len(),
        };
        let start = end.saturating_sub(limit);
        let page = settle(turns[start..end].to_vec(), turns, busy(app, bot));
        Ok((page, start > 0))
    }

    /// The bot's browser actions, newest first: what it did to which page, in
    /// which turn, and what started that turn.
    pub fn browser_activity(
        &self,
        app: &AppState,
        bot: &Bot,
        limit: usize,
    ) -> anyhow::Result<Vec<Value>> {
        self.refresh(app, bot)?;
        let (chat, _) = self.entry(app, bot)?;
        let chat = lock(&chat);
        let mut activity = Vec::new();
        for turn in chat.builder.turns.iter().rev() {
            let steps = turn.items.iter().rev().filter_map(|item| match item {
                super::model::ChatItem::Step(step) => {
                    super::browser_steps::browser_tool(&step.tool)
                        .map(|(browser, _)| (browser, step))
                }
                _ => None,
            });
            for (browser, step) in steps {
                activity.push(serde_json::json!({
                    "turn_id": turn.id, "step_id": step.id, "at": turn.started_at,
                    "browser": browser.as_str(), "title": step.title,
                    "subtitle": step.subtitle, "status": step.status, "trigger": turn.trigger
                }));
                if activity.len() >= limit {
                    return Ok(activity);
                }
            }
        }
        Ok(activity)
    }

    pub fn step(&self, app: &AppState, bot: &Bot, item_id: &str) -> anyhow::Result<StepDetail> {
        let (chat, _) = self.entry(app, bot)?;
        let chat = lock(&chat);
        let path = chat
            .path
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no transcript"))?;
        let locator = chat
            .builder
            .steps
            .get(item_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no step {item_id}"))?;
        drop(chat);
        let use_line: Value = serde_json::from_str(&line_at(&path, locator.use_offset)?)?;
        let call = use_line["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|b| b["id"] == item_id)
            .cloned()
            .unwrap_or(Value::Null);
        let input = &call["input"];
        let mut detail = StepDetail {
            input: Some(steps::truncate(
                &serde_json::to_string_pretty(input).unwrap_or_default(),
                MAX_INPUT_CHARS,
            )),
            command: input["command"].as_str().map(str::to_string),
            content: input["content"]
                .as_str()
                .map(|c| steps::truncate(c, MAX_CONTENT_CHARS)),
            ..StepDetail::default()
        };
        if let Some(offset) = locator.result_offset {
            let record: Value = serde_json::from_str(&line_at(&path, offset)?)?;
            let block = record["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|b| b["tool_use_id"] == item_id);
            detail.output = block
                .and_then(result_text)
                .map(|text| steps::tail(&text, MAX_OUTPUT_CHARS));
            detail.diff = diff_lines(&record["toolUseResult"]);
        }
        Ok(detail)
    }

    /// An image's media type and base64 bytes.
    pub fn image(
        &self,
        app: &AppState,
        bot: &Bot,
        image_id: &str,
    ) -> anyhow::Result<(String, String)> {
        let (chat, _) = self.entry(app, bot)?;
        let chat = lock(&chat);
        let path = chat
            .path
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no transcript"))?;
        let at = *chat
            .builder
            .images
            .get(image_id)
            .ok_or_else(|| anyhow::anyhow!("no image {image_id}"))?;
        drop(chat);
        let record: Value = serde_json::from_str(&line_at(&path, at.offset)?)?;
        let source = &record["message"]["content"][at.block]["content"][at.inner]["source"];
        let data = source["data"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("image bytes missing"))?;
        let mime = source["media_type"].as_str().unwrap_or("image/png");
        Ok((mime.to_string(), data.to_string()))
    }

    /// Bots whose chat has been loaded, for the watcher to keep current.
    pub fn loaded(&self) -> Vec<String> {
        self.bots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect()
    }

    /// The bot's chat, and whether it was created just now.
    fn entry(&self, app: &AppState, bot: &Bot) -> anyhow::Result<(Arc<Mutex<BotChat>>, bool)> {
        let mut bots = self.bots.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(chat) = bots.get(&bot.id) {
            return Ok((chat.clone(), false));
        }
        let chat = Arc::new(Mutex::new(BotChat {
            path: None,
            offset: 0,
            builder: Builder::new(&bot.id, names(app, bot)),
        }));
        bots.insert(bot.id.clone(), chat.clone());
        Ok((chat, true))
    }
}

fn lock(chat: &Mutex<BotChat>) -> std::sync::MutexGuard<'_, BotChat> {
    chat.lock().unwrap_or_else(|e| e.into_inner())
}

/// The file a bot's runtime records its conversation in, if it exists yet.
pub fn transcript(app: &AppState, bot: &Bot) -> Option<PathBuf> {
    if bot.is_linked() {
        return None;
    }
    let workspace = Path::new(&bot.workspace_path);
    match bot.runtime {
        BotRuntime::CodexCli => {
            let path = crate::runtime::codex::transcript_path(workspace);
            path.exists().then_some(path)
        }
        BotRuntime::ClaudeCode => crate::activity::newest_transcript(
            &crate::activity::transcript_dir(&app.cfg.user_home, workspace),
        ),
    }
}

/// Upper-case envelope names to real bot names, for the bot's project.
fn names(app: &AppState, bot: &Bot) -> HashMap<String, String> {
    app.db
        .list_bots(Some(&bot.project_id))
        .unwrap_or_default()
        .into_iter()
        .map(|b| (b.name.to_uppercase(), b.name))
        .collect()
}

/// Whether the bot could still be inside a turn. A turn whose bot is idle,
/// stopped or crashed is shown as finished even if its end was never written.
fn busy(app: &AppState, bot: &Bot) -> bool {
    matches!(
        app.supervisor.state(&bot.id).0,
        BotState::Working | BotState::WaitingForApproval | BotState::Starting
    )
}

fn settle(mut page: Vec<ChatTurn>, all: &[ChatTurn], busy: bool) -> Vec<ChatTurn> {
    let last = all.last().map(|t| t.id.as_str());
    for turn in &mut page {
        if turn.open && (!busy || Some(turn.id.as_str()) != last) {
            turn.open = false;
        }
    }
    page
}

/// Folds the complete lines after `offset` into the builder and returns the
/// offset of the first line not yet complete.
fn read_from(path: &Path, offset: u64, builder: &mut Builder) -> anyhow::Result<u64> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    // A shorter file than we have read is a rewritten one: start over.
    let offset = if len < offset { 0 } else { offset };
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let mut start = 0usize;
    while let Some(end) = bytes[start..].iter().position(|&b| b == b'\n') {
        let line = &bytes[start..start + end];
        if let Ok(text) = std::str::from_utf8(line) {
            builder.push_line(offset + start as u64, text);
        }
        start += end + 1;
    }
    Ok(offset + start as u64)
}

fn line_at(path: &Path, offset: u64) -> anyhow::Result<String> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut line = String::new();
    BufReader::new(file).read_line(&mut line)?;
    Ok(line)
}

/// A unified diff from Claude Code's structured patch, hunk headers included.
fn diff_lines(result: &Value) -> Option<Vec<String>> {
    let hunks = result["structuredPatch"]
        .as_array()
        .filter(|h| !h.is_empty())?;
    let mut lines = Vec::new();
    for hunk in hunks {
        let n = |key: &str| hunk[key].as_i64().unwrap_or(0);
        lines.push(format!(
            "@@ -{},{} +{},{} @@",
            n("oldStart"),
            n("oldLines"),
            n("newStart"),
            n("newLines")
        ));
        for line in hunk["lines"].as_array().into_iter().flatten() {
            lines.push(line.as_str().unwrap_or_default().to_string());
            if lines.len() >= MAX_DIFF_LINES {
                lines.push("… diff truncated".to_string());
                return Some(lines);
            }
        }
    }
    Some(lines)
}
