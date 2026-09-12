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
mod decisions;
mod decisions_publish;
mod dispatch;
mod entities;
mod messaging;
mod routines;
mod terminal;
mod views;

pub(crate) use views::{bot_view, project_view};

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
    /// None for the owner token; the issuing device otherwise. A ruling made
    /// from a device stays attributable after that device is revoked.
    device_id: Option<String>,
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
    let session = match stream.next().await {
        Some(Ok(WsMessage::Text(text))) => handshake(&app, &out_tx, &text),
        _ => None,
    };
    let Some((caps, device_id)) = session else {
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
        device_id,
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

/// Returns the authenticated connection's capability grants and issuing
/// device, or None when the handshake fails (an error frame is sent first).
fn handshake(
    app: &Arc<AppState>,
    out: &mpsc::UnboundedSender<Value>,
    text: &str,
) -> Option<(Vec<Capability>, Option<String>)> {
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
        "capabilities": crate::app::CAPABILITIES,
        "grants": cap_strs,
        "device_id": device_id
    }));
    Some((caps, device_id))
}
