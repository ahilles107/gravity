//! Last-activity summaries for the sidebar.
//!
//! A bot's real conversation happens in its Claude Code terminal, which is a
//! raw pty: nothing about it reaches the message bus. The bus only carries
//! routine tasks, bot-to-bot traffic and MCP sends, so it cannot answer "what
//! did this bot last say?" on its own.
//!
//! Claude Code does record every turn, in the JSONL transcript it keeps under
//! `~/.claude/projects/<mangled-workspace>/`. Reading the newest assistant turn
//! from there — and taking whichever of that and the bot's newest bus message
//! is more recent — is what makes the sidebar preview reflect reality.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use bus::SenderKind;

use crate::app::AppState;
use crate::db::Db;
use crate::events::{Internal, Push};

/// One line of preview for a bot: what was last said, by whom, and when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    /// Empty when the bot itself spoke; otherwise the sender's display name.
    pub from: String,
    pub text: String,
    pub at: DateTime<Utc>,
}

/// The wire shape of one preview line, shared by the `bot_activity` reply and
/// the `activity_update` push.
#[derive(Debug, Clone, Serialize)]
pub struct BotActivity {
    pub bot_id: String,
    pub from: String,
    pub text: String,
    /// RFC 3339.
    pub at: String,
}

impl Activity {
    pub fn for_wire(self, bot_id: &str) -> BotActivity {
        BotActivity {
            bot_id: bot_id.to_string(),
            from: self.from,
            text: self.text,
            at: self.at.to_rfc3339(),
        }
    }
}

/// How many trailing transcript lines to inspect. The newest assistant turn is
/// within a handful of lines of the end; the cap keeps a long session cheap.
pub(crate) const SCAN_LINES: usize = 400;

/// How much of a transcript's tail to read. Generous for `SCAN_LINES` of
/// JSONL, and a hard ceiling on what one scan allocates.
const TAIL_BYTES: u64 = 1 << 20;

/// Preview length. The client ellipsizes, so this only bounds what we ship.
const MAX_CHARS: usize = 200;

/// Claude Code's transcript directory for a workspace: the absolute path with
/// every `/` and `.` replaced by `-`, under `~/.claude/projects`.
pub(crate) fn transcript_dir(home: &Path, workspace: &Path) -> PathBuf {
    let mangled: String = workspace
        .to_string_lossy()
        .chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect();
    home.join(".claude").join("projects").join(mangled)
}

/// The most recently written `.jsonl` in `dir`, if any.
pub(crate) fn newest_transcript(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(seen, _)| modified > *seen) {
            best = Some((modified, path));
        }
    }
    best.map(|(_, path)| path)
}

/// The last `max_lines` lines of a transcript, newest first. An always-on
/// bot's session file grows without bound, so reading it whole to inspect its
/// end would cost more every day the bot lives; only the tail is read. The
/// seek can land mid-record, so that leading fragment is dropped.
pub(crate) fn tail_lines(path: &Path, max_lines: usize) -> Vec<String> {
    use std::io::{Read, Seek, SeekFrom};

    let Ok(mut file) = fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(len) = file.seek(SeekFrom::End(0)) else {
        return Vec::new();
    };
    let start = len.saturating_sub(TAIL_BYTES);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<&str> = text.lines().collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    lines
        .iter()
        .rev()
        .take(max_lines)
        .map(|line| (*line).to_string())
        .collect()
}

/// The text of an assistant entry, joining its text blocks; None for a turn
/// that only used tools.
fn assistant_text(entry: &Value) -> Option<String> {
    if entry.get("type").and_then(|v| v.as_str()) != Some("assistant") {
        return None;
    }
    let blocks = entry.get("message")?.get("content")?.as_array()?;
    let text: String = blocks
        .iter()
        .filter(|b| b.get("type").and_then(|v| v.as_str()) == Some("text"))
        .filter_map(|b| b.get("text").and_then(|v| v.as_str()))
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Newest assistant turn in a bot's Claude Code session, or None when the bot
/// has no transcript yet (never started, or never spoke).
pub fn from_transcript(home: &Path, workspace: &Path) -> Option<Activity> {
    let path = newest_transcript(&transcript_dir(home, workspace))?;
    for line in tail_lines(&path, SCAN_LINES) {
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(text) = assistant_text(&entry) else {
            continue;
        };
        let at = entry.get("timestamp").and_then(|v| v.as_str())?;
        let Ok(at) = DateTime::parse_from_rfc3339(at) else {
            continue;
        };
        return Some(Activity {
            from: String::new(),
            text: truncate(&text),
            at: at.with_timezone(&Utc),
        });
    }
    None
}

/// Newest bus message in a bot's DM. Covers routine tasks, bot-to-bot traffic
/// and MCP sends — everything the terminal transcript does not see.
fn from_bus(db: &Db, bot_id: &str) -> Option<Activity> {
    let conv = db.dm_conversation(bot_id).ok()??;
    let msg = db.list_messages(&conv.id, None, 1).ok()?.pop()?;
    let from = match msg.sender.kind {
        SenderKind::Bot if msg.sender.bot_id.as_deref() == Some(bot_id) => String::new(),
        _ => msg.sender.name.clone(),
    };
    Some(Activity {
        from,
        text: truncate(&msg.body),
        at: msg.created_at,
    })
}

/// One preview line for a bot: the newer of its last Claude Code turn and its
/// last bus message. None when the bot has said nothing yet.
pub fn for_bot(db: &Db, home: &Path, bot_id: &str, workspace: &Path) -> Option<Activity> {
    newer(from_transcript(home, workspace), from_bus(db, bot_id))
}

/// Collapses whitespace and caps the length, so one preview stays one line.
pub fn truncate(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(MAX_CHARS) {
        Some((idx, _)) => format!("{}…", &flat[..idx]),
        None => flat,
    }
}

/// The more recent of two candidates.
pub fn newer(a: Option<Activity>, b: Option<Activity>) -> Option<Activity> {
    match (a, b) {
        (Some(a), Some(b)) => {
            if b.at > a.at {
                Some(b)
            } else {
                Some(a)
            }
        }
        (a, b) => a.or(b),
    }
}

/// How long to keep waiting for a finished turn to reach the transcript.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(10);

/// How often to re-read while waiting.
const SETTLE_POLL: Duration = Duration::from_millis(100);

/// Pushes each bot's preview line once its finished turn is actually readable.
///
/// Claude Code's `Stop` hook fires *before* the turn is flushed to the
/// transcript — measured at roughly 250ms ahead of the write. A client that
/// reacts to the resulting `ready` state by re-reading therefore sees the
/// previous turn, and the sidebar sits one reply behind forever. So the wait
/// belongs here: poll until the transcript carries something at least as new as
/// the moment the turn ended, then push that.
pub async fn watch(app: Arc<AppState>) {
    use tokio::sync::broadcast::error::RecvError;

    let mut internal = app.events.subscribe_internal();
    loop {
        match internal.recv().await {
            Ok(Internal::BotDone { bot_id, .. }) => {
                tokio::spawn(settle_and_push(app.clone(), bot_id, Utc::now()));
            }
            // A burst of finished turns must not retire the watcher for the
            // rest of the daemon's life: the turns it skipped surface on the
            // next snapshot, the ones after it keep flowing.
            Err(RecvError::Lagged(skipped)) => {
                tracing::warn!(
                    skipped,
                    "activity watcher lagged; previews catch up on refresh"
                );
            }
            Err(RecvError::Closed) => break,
        }
    }
}

async fn settle_and_push(app: Arc<AppState>, bot_id: String, done_at: DateTime<Utc>) {
    let home = app.cfg.user_home.clone();
    let Ok(Some(bot)) = app.db.get_bot(&bot_id) else {
        return;
    };
    let workspace = PathBuf::from(&bot.workspace_path);
    let deadline = tokio::time::Instant::now() + SETTLE_TIMEOUT;
    loop {
        let found = for_bot(&app.db, &home, &bot_id, &workspace);
        // A turn that produced only tool calls never lands any text, so the
        // timeout is a normal outcome, not an error.
        if let Some(activity) = found {
            if activity.at >= done_at || tokio::time::Instant::now() >= deadline {
                app.events.push(Push::ActivityUpdate {
                    activity: activity.for_wire(&bot_id),
                });
                return;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(SETTLE_POLL).await;
    }
}

#[cfg(test)]
mod tests;
