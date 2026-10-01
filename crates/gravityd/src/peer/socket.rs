//! The two ends of a peer link: `/peer` accepts a dialing daemon, and a
//! dialer keeps a connection open to each peer this daemon was told to reach.
//! Both authenticate with a `peer_hello`, then hand the socket to the hub as a
//! pair of frame channels.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message as TungMessage;

use crate::app::AppState;

/// Version of the frames exchanged on a peer link.
const PEER_PROTOCOL: u64 = 1;

/// Largest frame on a peer link: a result whose artifacts are at the size
/// cap, base64-encoded, with room for the envelope.
const MAX_PEER_FRAME_BYTES: usize = 80 * 1024 * 1024;

const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const MIN_REDIAL: Duration = Duration::from_secs(2);
const MAX_REDIAL: Duration = Duration::from_secs(60);

pub async fn peer_handler(
    State(app): State<Arc<AppState>>,
    upgrade: WebSocketUpgrade,
) -> axum::response::Response {
    upgrade
        .max_message_size(MAX_PEER_FRAME_BYTES)
        .max_frame_size(MAX_PEER_FRAME_BYTES)
        .on_upgrade(move |socket| accept(app, socket))
        .into_response()
}

async fn accept(app: Arc<AppState>, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let hello = match tokio::time::timeout(HELLO_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(WsMessage::Text(text)))) => serde_json::from_str::<Value>(&text).ok(),
        _ => None,
    };
    let peer_id = match hello.as_ref().map(|hello| authenticate(&app, hello)) {
        Some(Ok(peer_id)) => peer_id,
        Some(Err(e)) => {
            tracing::warn!(error = %e, "peer handshake refused");
            let refusal = json!({ "type": "error", "message": e.to_string() });
            let _ = sink.send(WsMessage::Text(refusal.to_string())).await;
            return;
        }
        None => return,
    };
    let Ok(daemon_id) = app.db.daemon_id() else {
        return;
    };
    let ok = json!({ "type": "peer_hello_ok", "protocol": PEER_PROTOCOL, "daemon_id": daemon_id });
    if sink.send(WsMessage::Text(ok.to_string())).await.is_err() {
        return;
    }

    let (in_tx, in_rx) = mpsc::unbounded_channel();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            if sink.send(WsMessage::Text(frame.to_string())).await.is_err() {
                break;
            }
        }
    });
    let reader = tokio::spawn(async move {
        while let Some(Ok(frame)) = stream.next().await {
            match frame {
                WsMessage::Text(text) => {
                    if let Ok(value) = serde_json::from_str::<Value>(&text) {
                        if in_tx.send(value).is_err() {
                            break;
                        }
                    }
                }
                WsMessage::Close(_) => break,
                _ => {}
            }
        }
    });
    app.peers.serve(app.clone(), peer_id, in_rx, out_tx).await;
    reader.abort();
    writer.abort();
}

/// The peer a `peer_hello` authenticates as.
fn authenticate(app: &AppState, hello: &Value) -> anyhow::Result<String> {
    anyhow::ensure!(
        hello.get("type").and_then(Value::as_str) == Some("peer_hello"),
        "expected peer_hello"
    );
    let protocol = hello.get("protocol").and_then(Value::as_u64).unwrap_or(0);
    anyhow::ensure!(
        protocol == PEER_PROTOCOL,
        "this daemon speaks peer protocol {PEER_PROTOCOL}, not {protocol}"
    );
    let token = hello.get("token").and_then(Value::as_str).unwrap_or("");
    let daemon_id = hello
        .get("daemon_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow::anyhow!("peer_hello carries no daemon_id"))?;
    let peer_id = app
        .secrets
        .peer_for_token(token)
        .ok_or_else(|| anyhow::anyhow!("invalid peer token"))?;
    let peer = app
        .db
        .get_peer(&peer_id)?
        .filter(|p| p.revoked_at.is_none())
        .ok_or_else(|| anyhow::anyhow!("peer revoked"))?;
    anyhow::ensure!(
        app.db.bind_peer_daemon(&peer.id, daemon_id)?,
        "this peer token belongs to another daemon"
    );
    Ok(peer.id)
}

/// Starts a dialer for every peer this daemon reaches out to.
pub fn spawn_dialers(app: &Arc<AppState>) {
    match app.db.list_peers() {
        Ok(peers) => {
            for peer in peers {
                if peer.url.is_some() && peer.revoked_at.is_none() {
                    spawn_dialer(app.clone(), peer.id);
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not list peers to dial"),
    }
}

/// Keeps a link open to one peer until it is revoked.
pub fn spawn_dialer(app: Arc<AppState>, peer_id: String) {
    tokio::spawn(async move {
        let mut wait = MIN_REDIAL;
        loop {
            let peer = match app.db.get_peer(&peer_id) {
                Ok(Some(peer)) if peer.revoked_at.is_none() => peer,
                _ => return,
            };
            let (Some(url), Some(token)) = (peer.url.clone(), app.secrets.peer_token(&peer_id))
            else {
                return;
            };
            match dial(&app, &peer_id, &url, &token).await {
                Ok(()) => wait = MIN_REDIAL,
                Err(e) => {
                    tracing::debug!(peer = %peer.name, error = %format!("{e:#}"), "peer dial failed");
                }
            }
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(MAX_REDIAL);
        }
    });
}

/// One connection attempt, served until it drops. `Ok` once the link was up.
async fn dial(app: &Arc<AppState>, peer_id: &str, url: &str, token: &str) -> anyhow::Result<()> {
    let config = WebSocketConfig {
        max_message_size: Some(MAX_PEER_FRAME_BYTES),
        max_frame_size: Some(MAX_PEER_FRAME_BYTES),
        ..WebSocketConfig::default()
    };
    let (socket, _) = tokio_tungstenite::connect_async_with_config(url, Some(config), true).await?;
    let (mut sink, mut stream) = socket.split();
    let hello = json!({
        "type": "peer_hello",
        "protocol": PEER_PROTOCOL,
        "token": token,
        "daemon_id": app.db.daemon_id()?,
    });
    sink.send(TungMessage::Text(hello.to_string())).await?;
    let reply = match tokio::time::timeout(HELLO_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(TungMessage::Text(text)))) => serde_json::from_str::<Value>(&text)?,
        _ => anyhow::bail!("peer closed the link during the handshake"),
    };
    if reply.get("type").and_then(Value::as_str) != Some("peer_hello_ok") {
        let reason = reply
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("handshake refused");
        anyhow::bail!("{reason}");
    }
    let remote = reply
        .get("daemon_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("peer did not say which daemon it is"))?;
    anyhow::ensure!(
        app.db.bind_peer_daemon(peer_id, remote)?,
        "the daemon at {url} is not the one this peer was paired with"
    );

    let (in_tx, in_rx) = mpsc::unbounded_channel();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            if sink
                .send(TungMessage::Text(frame.to_string()))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    let reader = tokio::spawn(async move {
        while let Some(Ok(frame)) = stream.next().await {
            match frame {
                TungMessage::Text(text) => {
                    if let Ok(value) = serde_json::from_str::<Value>(&text) {
                        if in_tx.send(value).is_err() {
                            break;
                        }
                    }
                }
                TungMessage::Close(_) => break,
                _ => {}
            }
        }
    });
    app.peers
        .serve(app.clone(), peer_id.to_string(), in_rx, out_tx)
        .await;
    reader.abort();
    writer.abort();
    Ok(())
}
