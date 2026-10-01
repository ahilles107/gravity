//! Permission prompts answered from the app. A runtime asks — Claude Code
//! through its `PermissionRequest` hook (`claude`), Codex through its App
//! Server approval requests (`runtime`) — and the owner answers a card: allow
//! once, allow for this session, or deny. Nothing defaults to yes: an
//! unanswered prompt is denied when its window closes, and with no app able to
//! answer, the runtime is left to ask in its own terminal.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bus::BotState;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::oneshot;

use crate::app::AppState;
use crate::events::Push;

mod claude;
mod runtime;

pub use claude::permission_hook;
pub use runtime::watch;

/// How long Claude Code waits on the hook. Kept above any answer window the
/// config allows, so the daemon always answers before the hook is killed.
pub const HOOK_TIMEOUT_SECS: u64 = 900;
/// The longest answer window the config may ask for.
const MAX_WINDOW_SECS: u64 = HOOK_TIMEOUT_SECS - 60;
const MAX_INPUT_CHARS: usize = 4_000;

#[derive(Debug, Clone, Serialize)]
pub struct PermissionRequest {
    pub id: String,
    pub bot_id: String,
    pub tool: String,
    /// One line saying what the tool would do.
    pub summary: String,
    /// The tool input, pretty-printed and truncated.
    pub input: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    AllowOnce,
    AllowSession,
    Deny,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    AllowedOnce,
    AllowedSession,
    Denied,
    Expired,
    /// The asker went away before an answer: the runtime moved on without it.
    Abandoned,
}

/// How a prompt ended for the runtime that asked.
enum Decision {
    Answered(Answer, Option<String>),
    Expired,
}

struct Pending {
    request: PermissionRequest,
    /// Set for a runtime request, so the runtime can withdraw it.
    runtime_key: Option<u64>,
    reply: oneshot::Sender<(Answer, Option<String>)>,
}

#[derive(Default)]
pub struct Approvals {
    pending: Mutex<HashMap<String, Pending>>,
    /// Connected clients that show permission cards and may answer them.
    answerers: AtomicUsize,
}

/// Held by a connection that can answer permission cards; dropping it, when
/// the connection closes, takes that client out of the count.
pub struct Answerer(Arc<AppState>);

impl Drop for Answerer {
    fn drop(&mut self) {
        self.0.approvals.answerers.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Counts a connection in as able to answer permission cards.
pub fn answerer(app: &Arc<AppState>) -> Answerer {
    app.approvals.answerers.fetch_add(1, Ordering::SeqCst);
    Answerer(app.clone())
}

impl Approvals {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Pending>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn list(&self, bot_id: Option<&str>) -> Vec<PermissionRequest> {
        let mut out: Vec<PermissionRequest> = self
            .lock()
            .values()
            .map(|p| p.request.clone())
            .filter(|r| bot_id.is_none_or(|id| r.bot_id == id))
            .collect();
        out.sort_by_key(|r| r.created_at);
        out
    }

    /// Answers a pending prompt. Errors when it is no longer pending.
    pub fn answer(
        &self,
        request_id: &str,
        answer: Answer,
        reason: Option<String>,
    ) -> anyhow::Result<PermissionRequest> {
        let pending = self
            .lock()
            .remove(request_id)
            .ok_or_else(|| anyhow::anyhow!("that permission prompt is no longer waiting"))?;
        let request = pending.request.clone();
        // A closed receiver means the asker just gave up; the prompt is gone
        // either way, so the answer has nothing left to decide.
        let _ = pending.reply.send((answer, reason));
        Ok(request)
    }

    /// Withdraws a runtime's request that was settled somewhere else.
    fn withdraw(&self, bot_id: &str, key: u64) {
        self.lock()
            .retain(|_, p| !(p.request.bot_id == bot_id && p.runtime_key == Some(key)));
    }
}

/// Removes a prompt that is no longer waiting, whichever way it ended, and
/// tells clients.
struct Settle<'a> {
    app: &'a AppState,
    request: PermissionRequest,
    outcome: Option<Outcome>,
}

impl Drop for Settle<'_> {
    fn drop(&mut self) {
        self.app.approvals.lock().remove(&self.request.id);
        let outcome = self.outcome.unwrap_or(Outcome::Abandoned);
        self.app.events.push(Push::PermissionResolved {
            request_id: self.request.id.clone(),
            bot_id: self.request.bot_id.clone(),
            outcome,
        });
        if self.app.supervisor.state(&self.request.bot_id).0 == BotState::WaitingForApproval
            && self
                .app
                .approvals
                .list(Some(&self.request.bot_id))
                .is_empty()
        {
            self.app.supervisor.set_state(
                &self.request.bot_id,
                BotState::Working,
                "permission answered",
            );
        }
    }
}

/// Shows the owner a card and waits for the answer, or for the window to
/// close. `None` when no app can answer, or the prompt went away first: the
/// runtime then asks in its own terminal.
async fn decide(
    app: &AppState,
    bot_id: &str,
    tool: &str,
    input: &Value,
    runtime_key: Option<u64>,
) -> Option<Decision> {
    // No app that could answer is open: the prompt belongs in the terminal,
    // exactly as it was before cards existed.
    if app.approvals.answerers.load(Ordering::SeqCst) == 0 {
        return None;
    }
    let window = Duration::from_secs(app.cfg.permission_timeout_seconds.clamp(1, MAX_WINDOW_SECS));
    let created_at = Utc::now();
    let request = PermissionRequest {
        id: bus::new_id(),
        bot_id: bot_id.to_string(),
        tool: tool.to_string(),
        summary: summary(tool, input),
        input: crate::chat::truncate(
            &serde_json::to_string_pretty(input).unwrap_or_default(),
            MAX_INPUT_CHARS,
        ),
        created_at,
        expires_at: created_at + chrono::Duration::from_std(window).ok()?,
    };
    let (tx, rx) = oneshot::channel();
    app.approvals.lock().insert(
        request.id.clone(),
        Pending {
            request: request.clone(),
            runtime_key,
            reply: tx,
        },
    );
    let mut settle = Settle {
        app,
        request: request.clone(),
        outcome: None,
    };
    app.supervisor
        .set_state(bot_id, BotState::WaitingForApproval, &request.summary);
    app.events.push(Push::PermissionRequest { request });

    match tokio::time::timeout(window, rx).await {
        Ok(Ok((answer, reason))) => {
            settle.outcome = Some(match answer {
                Answer::AllowOnce => Outcome::AllowedOnce,
                Answer::AllowSession => Outcome::AllowedSession,
                Answer::Deny => Outcome::Denied,
            });
            Some(Decision::Answered(answer, reason))
        }
        Ok(Err(_)) => None,
        Err(_) => {
            settle.outcome = Some(Outcome::Expired);
            Some(Decision::Expired)
        }
    }
}

/// One line for the card: the command, the file, or the tool's own name.
fn summary(tool: &str, input: &Value) -> String {
    let field = |key: &str| input[key].as_str().filter(|v| !v.is_empty());
    let detail = field("command")
        .or_else(|| field("file_path"))
        .or_else(|| field("url"))
        .or_else(|| field("path"))
        .or_else(|| field("description"))
        .or_else(|| field("reason"));
    match detail {
        Some(detail) => format!(
            "{tool}: {}",
            crate::chat::truncate(detail.lines().next().unwrap_or(detail), 200)
        ),
        None => tool.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summarises_the_prompt() {
        assert_eq!(
            summary("Bash", &json!({"command": "rm -rf build\necho done"})),
            "Bash: rm -rf build"
        );
        assert_eq!(
            summary("Write", &json!({"file_path": "/w/a.txt"})),
            "Write: /w/a.txt"
        );
        assert_eq!(summary("mcp__x__y", &json!({})), "mcp__x__y");
    }
}
