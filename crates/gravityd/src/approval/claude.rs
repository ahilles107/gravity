//! Claude Code's side of permission prompts: the `PermissionRequest` hook
//! posts the prompt and waits for the decision it prints back.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::{json, Value};

use crate::app::AppState;

use super::{decide, Answer, Decision};

/// POST /hook/permission — answers with the hook output that decides the
/// prompt, or an empty body to leave the prompt to the terminal.
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
    let tool = body["tool_name"].as_str().unwrap_or("a tool").to_string();
    let output = match decide(&app, &bot_id, &tool, &body["tool_input"], None).await {
        Some(Decision::Answered(Answer::AllowOnce, _)) => allow(None),
        Some(Decision::Answered(Answer::AllowSession, _)) => {
            allow(Some(session_rules(&tool, &body)))
        }
        Some(Decision::Answered(Answer::Deny, reason)) => {
            deny(reason.as_deref().unwrap_or("The owner denied this."))
        }
        Some(Decision::Expired) => deny("The owner did not answer this permission prompt in time."),
        None => return StatusCode::OK.into_response(),
    };
    (StatusCode::OK, Json(output)).into_response()
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
fn session_rules(tool: &str, body: &Value) -> Vec<Value> {
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
}
