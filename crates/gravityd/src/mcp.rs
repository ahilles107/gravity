//! Bus MCP server: one Streamable-HTTP endpoint for all bots, with
//! least-privilege identity per connection (Bearer bot token). Also hosts the
//! lifecycle hook endpoint used by Claude Code hook commands.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use bus::{Sender, SenderKind, Trigger};
use serde_json::{json, Value};

use crate::app::AppState;

mod routines;
mod schema;
mod selfmgmt;
mod tasks;
mod tools;

use routines::{
    create_routine, delete_routine, emit_signal, list_routines, set_routine_enabled, update_routine,
};
use schema::tool_list;
use selfmgmt::{create_bot, delete_bot, get_self, rename_self, update_bot, update_self};
use tasks::{cancel_task, complete_task};
use tools::{check_inbox, list_bots, send_message};

fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Query string of a hook POST. The Notification hook forwards Claude Code's
/// raw stdin payload as the body, so its event name travels here instead.
#[derive(serde::Deserialize)]
pub struct HookQuery {
    #[serde(default)]
    event: String,
}

/// POST /hook — lifecycle events from Claude Code hook commands.
pub async fn hook_handler(
    State(app): State<Arc<AppState>>,
    Query(query): Query<HookQuery>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let Some(token) = bearer(&headers) else {
        return StatusCode::UNAUTHORIZED;
    };
    let Some(bot_id) = app.secrets.bot_for_token(&token) else {
        return StatusCode::UNAUTHORIZED;
    };
    let event = if query.event.is_empty() {
        body.get("event")
            .or_else(|| body.get("hook_event_name"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
    } else {
        query.event.as_str()
    };
    if event.is_empty() {
        return StatusCode::BAD_REQUEST;
    }
    // SessionStart reports the session's inbox socket for channel delivery.
    if let Some(socket) = body.get("socket").and_then(|v| v.as_str()) {
        let msg_token = body.get("msg_token").and_then(|v| v.as_str());
        app.supervisor.set_msg_socket(&bot_id, socket, msg_token);
    }
    let message = body.get("message").and_then(|v| v.as_str());
    let transcript_path = body.get("transcript_path").and_then(|v| v.as_str());
    app.supervisor
        .on_hook_with_transcript(&bot_id, event, message, transcript_path);
    StatusCode::OK
}

/// POST /mcp — JSON-RPC 2.0 (MCP Streamable HTTP, request/response subset).
pub async fn mcp_handler(
    State(app): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<Value>,
) -> impl IntoResponse {
    let Some(token) = bearer(&headers) else {
        return (StatusCode::UNAUTHORIZED, Json(json!({}))).into_response();
    };
    let Some(bot_id) = app.secrets.bot_for_token(&token) else {
        return (StatusCode::UNAUTHORIZED, Json(json!({}))).into_response();
    };

    let id = req.get("id").cloned();
    let method = req
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or_default()
        .to_string();
    // Notifications get no response body.
    if id.is_none() {
        return StatusCode::ACCEPTED.into_response();
    }
    let params = req.get("params").cloned().unwrap_or(json!({}));

    let result = match method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "gravity-bus", "version": crate::app::DAEMON_VERSION }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tool_list()),
        // Tool-level failures travel as `isError` content; an Err here means
        // the request itself was malformed, which is invalid params (-32602).
        "tools/call" => tool_call(&app, &bot_id, &params).map_err(|msg| (-32602, msg)),
        _ => Err((-32601, format!("method not found: {method}"))),
    };

    let body = match result {
        Ok(res) => json!({ "jsonrpc": "2.0", "id": id, "result": res }),
        Err((code, msg)) => json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": code, "message": msg }
        }),
    };
    (StatusCode::OK, Json(body)).into_response()
}

fn text_result(v: &Value) -> Value {
    json!({ "content": [{ "type": "text", "text": v.to_string() }] })
}

fn tool_error(msg: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": msg }], "isError": true })
}

fn tool_call(app: &Arc<AppState>, bot_id: &str, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or("missing tool name")?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let out = match name {
        "send_message" => send_message(app, bot_id, &args),
        "complete_task" => complete_task(app, bot_id, &args),
        "cancel_task" => cancel_task(app, bot_id, &args),
        "list_bots" => list_bots(app, bot_id),
        "check_inbox" => check_inbox(app, bot_id),
        "create_routine" => create_routine(app, bot_id, &args),
        "list_routines" => list_routines(app, bot_id),
        "set_routine_enabled" => set_routine_enabled(app, bot_id, &args),
        "update_routine" => update_routine(app, bot_id, &args),
        "delete_routine" => delete_routine(app, bot_id, &args),
        "emit_signal" => emit_signal(app, bot_id, &args),
        "get_self" => get_self(app, bot_id),
        "update_self" => update_self(app, bot_id, &args),
        "rename_self" => rename_self(app, bot_id, &args),
        "create_bot" => create_bot(app, bot_id, &args),
        "update_bot" => update_bot(app, bot_id, &args),
        "delete_bot" => delete_bot(app, bot_id, &args),
        other => Err(anyhow::anyhow!("unknown tool: {other}")),
    };
    Ok(match out {
        Ok(v) => text_result(&v),
        Err(e) => tool_error(&e.to_string()),
    })
}

fn caller(app: &Arc<AppState>, bot_id: &str) -> anyhow::Result<bus::Bot> {
    app.db
        .get_bot(bot_id)?
        .ok_or_else(|| anyhow::anyhow!("caller bot not found"))
}

fn bot_sender(bot: &bus::Bot) -> Sender {
    Sender {
        kind: SenderKind::Bot,
        bot_id: Some(bot.id.clone()),
        name: bot.name.clone(),
    }
}

/// Longest allowed interval. Also keeps `seconds` far inside the range where
/// chrono duration arithmetic is safe — an unbounded value would panic the
/// scheduler task and stop every routine on the daemon.
const MAX_INTERVAL_SECONDS: u64 = 366 * 24 * 60 * 60;
/// Minimum spacing between occurrences, for interval and cron alike.
const MIN_TRIGGER_GAP_SECONDS: i64 = 60;

/// Validate a trigger against the schedule floor and, for signal triggers,
/// against the owning bot's project so a routine can never watch a bot it
/// could not otherwise see.
pub fn validate_trigger(
    db: &crate::db::Db,
    project_id: &str,
    owner_bot_id: &str,
    trigger: &Trigger,
) -> anyhow::Result<()> {
    match trigger {
        Trigger::Cron { expr, tz } => {
            use std::str::FromStr;
            let schedule = cron::Schedule::from_str(expr)
                .map_err(|e| anyhow::anyhow!("invalid cron expression: {e}"))?;
            let tz: chrono_tz::Tz = tz
                .parse()
                .map_err(|_| anyhow::anyhow!("unknown timezone: {tz}"))?;
            // The cron grammar has a seconds field, so "every second" parses
            // fine; sample upcoming occurrences to hold the 60s floor.
            let times: Vec<_> = schedule
                .after(&chrono::Utc::now().with_timezone(&tz))
                .take(4)
                .collect();
            for pair in times.windows(2) {
                if (pair[1] - pair[0]).num_seconds() < MIN_TRIGGER_GAP_SECONDS {
                    anyhow::bail!(
                        "cron schedule fires more often than once per \
                         {MIN_TRIGGER_GAP_SECONDS} seconds"
                    );
                }
            }
        }
        Trigger::Interval { seconds } => {
            if *seconds < MIN_TRIGGER_GAP_SECONDS as u64 || *seconds > MAX_INTERVAL_SECONDS {
                anyhow::bail!(
                    "interval must be between {MIN_TRIGGER_GAP_SECONDS} seconds \
                     and {MAX_INTERVAL_SECONDS} seconds (one year)"
                );
            }
        }
        Trigger::Signal { name, from_bot_id } => {
            crate::scheduler::validate_signal_name(name)?;
            if let Some(from) = from_bot_id {
                if from == owner_bot_id {
                    anyhow::bail!(
                        "a routine never triggers on its own bot's signals; \
                         pick another bot or omit from_bot_id"
                    );
                }
                let known = db
                    .get_live_bot(from)?
                    .is_some_and(|b| b.project_id == project_id);
                if !known {
                    anyhow::bail!(
                        "unknown from_bot_id '{from}': use an id from list_bots \
                         in your project"
                    );
                }
            }
        }
    }
    Ok(())
}
