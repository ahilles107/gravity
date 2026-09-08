//! Versioned WebSocket control plane: handshake, request/response with
//! `req_id`, server pushes, terminal attach with replay cursors, and input
//! grant-gated typing. See `docs/protocol.md`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use bus::Capability;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::app::{AppState, DAEMON_VERSION, PROTOCOL_VERSION};

mod admin;
mod entities;
mod messaging;
mod routines;
mod terminal;

const MAX_FRAME_BYTES: usize = 1024 * 1024;

pub async fn ws_handler(
    State(app): State<Arc<AppState>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> axum::response::Response {
    if !origin_allowed(&app, &headers) {
        return (
            axum::http::StatusCode::FORBIDDEN,
            "origin not allowed".to_string(),
        )
            .into_response();
    }
    upgrade
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| handle_socket(app, socket))
        .into_response()
}

/// Native clients send no Origin header; browser contexts must match the
/// localhost/tauri defaults or the configured allowlist.
fn origin_allowed(app: &Arc<AppState>, headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) else {
        return true;
    };
    if origin == "null"
        || origin.starts_with("tauri://")
        || origin.starts_with("http://tauri.")
        || origin.starts_with("http://localhost")
        || origin.starts_with("http://127.0.0.1")
    {
        return true;
    }
    app.cfg.allowed_origins.iter().any(|o| o == origin)
}

struct Conn {
    app: Arc<AppState>,
    out: mpsc::UnboundedSender<Value>,
    /// bot_id -> forwarding task for live terminal frames.
    attachments: HashMap<String, JoinHandle<()>>,
    caps: Vec<Capability>,
}

async fn handle_socket(app: Arc<AppState>, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Value>();

    // Writer task: serialize all outbound frames through one channel.
    let writer = tokio::spawn(async move {
        while let Some(v) = out_rx.recv().await {
            let text = v.to_string();
            if sink.send(WsMessage::Text(text)).await.is_err() {
                break;
            }
        }
    });

    // Handshake: first frame must be a valid hello.
    let caps = match stream.next().await {
        Some(Ok(WsMessage::Text(text))) => handshake(&app, &out_tx, &text),
        _ => None,
    };
    let Some(caps) = caps else {
        drop(out_tx);
        let _ = writer.await;
        return;
    };

    // Forward server pushes to this client.
    let push_tx = out_tx.clone();
    let mut push_rx = app.events.subscribe_push();
    let push_app = app.clone();
    let push_task = tokio::spawn(async move {
        loop {
            let push = match push_rx.recv().await {
                Ok(push) => push,
                // Falling behind a burst must not end the feed for good: the
                // client keeps the pushes that follow and resyncs the rest on
                // its own. Ending the loop here left a connection silently
                // stale until it reconnected.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "client push feed lagged; some pushes were dropped");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            // `BotUpdated` carries a database row, whose `state` and
            // `unread_count` are placeholders the supervisor normally overlays.
            // Serialising it raw would tell clients every changed bot is
            // stopped with nothing unread, so it is rendered the same way a
            // `list_bots` reply is.
            let value = match &push {
                crate::events::Push::BotUpdated { bot } => {
                    Ok(json!({ "type": "bot_updated", "bot": bot_view(&push_app, bot) }))
                }
                // Archiving tombstones the name so it can be reused; clients
                // should see the name the project actually had.
                crate::events::Push::ProjectUpdated { project } => {
                    Ok(json!({ "type": "project_updated", "project": project_view(project) }))
                }
                other => serde_json::to_value(other),
            };
            if let Ok(v) = value {
                if push_tx.send(v).is_err() {
                    break;
                }
            }
        }
    });

    let mut conn = Conn {
        app: app.clone(),
        out: out_tx.clone(),
        attachments: HashMap::new(),
        caps,
    };

    while let Some(Ok(frame)) = stream.next().await {
        match frame {
            WsMessage::Text(text) => {
                if text.len() > MAX_FRAME_BYTES {
                    continue;
                }
                let Ok(req) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                conn.dispatch(&req);
            }
            WsMessage::Close(_) => break,
            _ => {}
        }
    }

    // Cleanup: attachments die with the connection.
    for (_, task) in conn.attachments.drain() {
        task.abort();
    }
    push_task.abort();
    drop(out_tx);
    drop(conn);
    let _ = writer.await;
}

/// Returns the authenticated connection's capability grants, or None when the
/// handshake fails (an error frame is sent first).
fn handshake(
    app: &Arc<AppState>,
    out: &mpsc::UnboundedSender<Value>,
    text: &str,
) -> Option<Vec<Capability>> {
    let Ok(req) = serde_json::from_str::<Value>(text) else {
        return None;
    };
    let req_id = req.get("req_id").cloned().unwrap_or(Value::Null);
    let fail = |code: &str, message: String| {
        let _ = out.send(json!({
            "type": "error", "req_id": req_id.clone(), "code": code, "message": message
        }));
    };
    if req.get("type").and_then(|t| t.as_str()) != Some("hello") {
        fail("invalid_request", "expected hello".to_string());
        return None;
    }
    let version = req
        .get("protocol_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if version != PROTOCOL_VERSION as u64 {
        fail(
            "unsupported_version",
            format!("server speaks protocol {PROTOCOL_VERSION}"),
        );
        return None;
    }
    let token = req.get("token").and_then(|t| t.as_str()).unwrap_or("");

    // The owner token (same machine, mode 0600) grants everything; device
    // tokens carry explicit scoped capabilities and can be revoked.
    let (caps, device_id) = if app.secrets.verify_client(token) {
        (
            vec![Capability::Read, Capability::Control, Capability::Approve],
            None,
        )
    } else if let Some(device_id) = app.secrets.device_for_token(token) {
        match app.db.get_device(&device_id) {
            Ok(Some(d)) if d.revoked_at.is_none() => (d.capabilities, Some(device_id)),
            _ => {
                fail("auth_failed", "device credential revoked".to_string());
                return None;
            }
        }
    } else {
        fail("auth_failed", "invalid client token".to_string());
        return None;
    };
    if let Some(id) = &device_id {
        let _ = app.db.touch_device(id);
    }
    let cap_strs: Vec<&str> = caps.iter().map(|c| c.as_str()).collect();
    let _ = out.send(json!({
        "type": "hello_ok", "req_id": req_id,
        "protocol_version": PROTOCOL_VERSION,
        "server_version": DAEMON_VERSION,
        "capabilities": ["terminal_attach", "search", "routines", "devices",
                         "bot_self_management", "config"],
        "grants": cap_strs,
        "device_id": device_id
    }));
    Some(caps)
}

/// Capability required for each request type. Everything not listed as
/// read-only requires `control`.
fn required_cap(kind: &str) -> Capability {
    match kind {
        "list_projects" | "list_bots" | "list_messages" | "list_conversations"
        | "list_routines" | "list_routine_runs" | "list_deliveries" | "list_devices" | "search"
        | "diagnostics" | "get_config" | "attach" | "detach" | "list_bot_revisions"
        | "list_bot_activity" => Capability::Read,
        _ => Capability::Control,
    }
}

impl Conn {
    pub(super) fn send(&self, v: Value) {
        let _ = self.out.send(v);
    }

    pub(super) fn reply_err(&self, req_id: &Value, code: &str, message: &str) {
        self.send(json!({
            "type": "error", "req_id": req_id, "code": code, "message": message
        }));
    }

    fn dispatch(&mut self, req: &Value) {
        let req_id = req.get("req_id").cloned().unwrap_or(Value::Null);
        let kind = req.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if !self.caps.contains(&required_cap(kind)) {
            self.reply_err(
                &req_id,
                "forbidden",
                &format!(
                    "'{kind}' requires the {} capability",
                    required_cap(kind).as_str()
                ),
            );
            return;
        }
        let result = match kind {
            "list_projects" => self.list_projects(&req_id),
            "create_project" => self.create_project(&req_id, req),
            "update_project" => self.update_project(&req_id, req),
            "delete_project" => self.delete_project(&req_id, req),
            "list_bots" => self.list_bots(&req_id, req),
            "list_bot_activity" => self.list_bot_activity(&req_id, req),
            "create_bot" => self.create_bot(&req_id, req),
            "update_bot" => self.update_bot(&req_id, req),
            "delete_bot" => self.delete_bot(&req_id, req),
            "list_bot_revisions" => self.list_bot_revisions(&req_id, req),
            "revert_bot_revision" => self.revert_bot_revision(&req_id, req),
            "attach" => self.attach(&req_id, req),
            "detach" => self.detach(&req_id, req),
            "input" => self.input(req),
            "resize" => self.resize(req),
            "send_user_message" => self.send_user_message(&req_id, req),
            "list_messages" => self.list_messages(&req_id, req),
            "list_conversations" => self.list_conversations(&req_id, req),
            "list_routines" => self.list_routines(&req_id, req),
            "create_routine" => self.create_routine(&req_id, req),
            "set_routine_enabled" => self.set_routine_enabled(&req_id, req),
            "run_routine_now" => self.run_routine_now(&req_id, req),
            "cancel_routine_run" => self.cancel_routine_run(&req_id, req),
            "emit_signal" => self.emit_signal(&req_id, req),
            "list_routine_runs" => self.list_routine_runs(&req_id, req),
            "list_deliveries" => self.list_deliveries(&req_id, req),
            "retry_delivery" => self.retry_delivery(&req_id, req),
            "search" => self.search(&req_id, req),
            "diagnostics" => self.diagnostics(&req_id),
            "get_config" => self.get_config(&req_id),
            "set_config" => self.set_config(&req_id, req),
            "list_devices" => self.list_devices(&req_id),
            "create_device" => self.create_device(&req_id, req),
            "revoke_device" => self.revoke_device(&req_id, req),
            other => {
                self.reply_err(
                    &req_id,
                    "invalid_request",
                    &format!("unknown type: {other}"),
                );
                Ok(())
            }
        };
        if let Err(e) = result {
            self.reply_err(&req_id, "internal", &e.to_string());
        }
    }

    pub(super) fn str_field<'a>(req: &'a Value, field: &str) -> anyhow::Result<&'a str> {
        req.get(field)
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'{field}' is required"))
    }

    pub(super) fn bot_json(&self, bot: &bus::Bot) -> Value {
        bot_view(&self.app, bot)
    }
}

/// A project as clients see it, with an archived row's original name restored.
pub(crate) fn project_view(project: &bus::Project) -> Value {
    json!({
        "id": project.id,
        "name": crate::db::Db::display_project_name(project),
        "dir_name": project.dir_name,
        "deleted_at": project.deleted_at.map(|t| t.to_rfc3339()),
        "created_at": project.created_at.to_rfc3339()
    })
}

/// A bot as clients see it: the stored row plus the runtime fields the
/// supervisor and the delivery tables own. Every path that hands a bot to a
/// client goes through here, replies and pushes alike, so the two cannot drift.
pub(crate) fn bot_view(app: &AppState, bot: &bus::Bot) -> Value {
    let (state, reason) = app.supervisor.state(&bot.id);
    let unread = app.db.unread_count(&bot.id).unwrap_or(0);
    json!({
            "id": bot.id,
            "project_id": bot.project_id,
            // Archived rows carry a tombstoned name so the original is free to
            // reuse; clients should always see the name the bot actually had.
            "name": crate::db::Db::display_name(bot),
            "description": bot.description,
            "avatar": bot.avatar,
            "instructions": bot.instructions,
            "state": state.as_str(),
            "state_reason": reason,
            "unread_count": unread,
            "workspace_path": bot.workspace_path,
            "dir_name": bot.dir_name,
            "created_by_bot_id": bot.created_by_bot_id,
            "deleted_at": bot.deleted_at.map(|t| t.to_rfc3339()),
        "created_at": bot.created_at.to_rfc3339()
    })
}
