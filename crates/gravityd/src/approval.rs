//! Permission prompts answered from the app. Claude Code's `PermissionRequest`
//! hook posts the prompt here and waits; the owner answers the card (allow
//! once, allow for this session, deny) and the answer goes back as the hook's
//! decision. Nothing defaults to yes: an unanswered prompt is denied when its
//! window closes, and a daemon that cannot answer leaves Claude Code to show
//! its own prompt in the terminal.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use bus::BotState;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::app::AppState;
use crate::events::Push;

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
    /// The hook went away before an answer: Claude Code moved on without it.
    Abandoned,
}

struct Pending {
    request: PermissionRequest,
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
        // A closed receiver means the hook just gave up; the prompt is gone
        // either way, so the answer has nothing left to decide.
        let _ = pending.reply.send((answer, reason));
        Ok(request)
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

/// POST /hook/permission — Claude Code's `PermissionRequest` hook. Answers
/// with the hook output that decides the prompt, or an empty body to leave
/// the prompt to the terminal.
pub async fn permission_hook(
    State(app): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let bot = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .and_then(|token| app.secrets.bot_for_token(token));
    let Some(bot_id) = bot else {
        return (StatusCode::UNAUTHORIZED, Json(Value::Null)).into_response();
    };
    match ask(&app, &bot_id, &body).await {
        Some(output) => (StatusCode::OK, Json(output)).into_response(),
        None => StatusCode::OK.into_response(),
    }
}

async fn ask(app: &AppState, bot_id: &str, body: &Value) -> Option<Value> {
    // No app that could answer is open: the prompt belongs in the terminal,
    // exactly as it was before cards existed.
    if app.approvals.answerers.load(Ordering::SeqCst) == 0 {
        return None;
    }
    let tool = body["tool_name"].as_str().unwrap_or("a tool").to_string();
    let input = &body["tool_input"];
    let window = Duration::from_secs(app.cfg.permission_timeout_seconds.clamp(1, MAX_WINDOW_SECS));
    let created_at = Utc::now();
    let request = PermissionRequest {
        id: bus::new_id(),
        bot_id: bot_id.to_string(),
        summary: summary(&tool, input),
        input: crate::chat::truncate(
            &serde_json::to_string_pretty(input).unwrap_or_default(),
            MAX_INPUT_CHARS,
        ),
        tool,
        created_at,
        expires_at: created_at + chrono::Duration::from_std(window).ok()?,
    };
    let (tx, rx) = oneshot::channel();
    app.approvals.lock().insert(
        request.id.clone(),
        Pending {
            request: request.clone(),
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
    app.events.push(Push::PermissionRequest {
        request: request.clone(),
    });

    let (answer, reason) = match tokio::time::timeout(window, rx).await {
        Ok(Ok(answered)) => answered,
        Ok(Err(_)) => return None,
        Err(_) => {
            settle.outcome = Some(Outcome::Expired);
            return Some(deny(
                "The owner did not answer this permission prompt in time.",
            ));
        }
    };
    let (outcome, output) = match answer {
        Answer::AllowOnce => (Outcome::AllowedOnce, allow(None)),
        Answer::AllowSession => (
            Outcome::AllowedSession,
            allow(Some(session_rules(&request.tool, body))),
        ),
        Answer::Deny => (
            Outcome::Denied,
            deny(reason.as_deref().unwrap_or("The owner denied this.")),
        ),
    };
    settle.outcome = Some(outcome);
    Some(output)
}

fn allow(updated_permissions: Option<Vec<Value>>) -> Value {
    let mut decision = json!({ "behavior": "allow" });
    if let Some(rules) = updated_permissions.filter(|r| !r.is_empty()) {
        decision["updatedPermissions"] = json!(rules);
    }
    json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } })
}

fn deny(message: &str) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": { "behavior": "deny", "message": message }
        }
    })
}

/// The rules "allow for this session" adds: Claude Code's own suggestions,
/// confined to the session, or a rule for the tool when it suggested none.
/// Mode changes and anything persistent are dropped: nothing this answer
/// grants outlives the session.
pub fn session_rules(tool: &str, body: &Value) -> Vec<Value> {
    let suggested: Vec<Value> = body["permission_suggestions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| matches!(s["type"].as_str(), Some("addRules" | "addDirectories")))
        .map(|s| {
            let mut s = s.clone();
            s["destination"] = json!("session");
            s
        })
        .collect();
    if !suggested.is_empty() {
        return suggested;
    }
    vec![json!({
        "type": "addRules",
        "rules": [{ "toolName": tool }],
        "behavior": "allow",
        "destination": "session"
    })]
}

/// One line for the card: the command, the file, or the tool's own name.
fn summary(tool: &str, input: &Value) -> String {
    let field = |key: &str| input[key].as_str().filter(|v| !v.is_empty());
    let detail = field("command")
        .or_else(|| field("file_path"))
        .or_else(|| field("url"))
        .or_else(|| field("path"))
        .or_else(|| field("description"));
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

    #[test]
    fn session_rules_never_persist() {
        let body = json!({ "permission_suggestions": [
            {"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": "npm test"}],
             "behavior": "allow", "destination": "localSettings"},
            {"type": "setMode", "mode": "acceptEdits", "destination": "session"}
        ]});
        let rules = session_rules("Bash", &body);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["destination"], "session");
        assert_eq!(rules[0]["rules"][0]["ruleContent"], "npm test");

        let fallback = session_rules("WebFetch", &json!({}));
        assert_eq!(fallback[0]["rules"][0]["toolName"], "WebFetch");
        assert_eq!(fallback[0]["destination"], "session");
    }

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
