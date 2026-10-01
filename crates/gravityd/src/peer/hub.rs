//! Live peer links. Each paired daemon has at most one connection, whichever
//! side dialed it; requests go out over it and wait for the matching response.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot, Notify};

use crate::app::AppState;

/// How long a request waits for the peer's response. A forwarded message with
/// artifacts is the slowest frame there is, and a tailnet hop is fast.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Why a request to a peer did not produce a result.
#[derive(Debug)]
pub enum PeerError {
    /// No live link. Nothing was sent, so retrying later is safe.
    Offline,
    /// The peer answered with a refusal, or never answered.
    Rejected(String),
    /// The peer refused with a code a client can act on (`conflict`,
    /// `not_linked`, `not_found`), carried from a [`super::Refusal`].
    Refused { code: String, reason: String },
}

impl PeerError {
    /// The client-facing error code: the peer's own when it gave one.
    pub fn code(&self) -> &str {
        match self {
            PeerError::Offline => "unavailable",
            PeerError::Rejected(_) => "invalid_request",
            PeerError::Refused { code, .. } => code,
        }
    }
}

impl std::fmt::Display for PeerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerError::Offline => write!(f, "peer is offline"),
            PeerError::Rejected(reason) | PeerError::Refused { reason, .. } => {
                write!(f, "{reason}")
            }
        }
    }
}

impl std::error::Error for PeerError {}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

struct Link {
    out: mpsc::UnboundedSender<Value>,
    pending: Pending,
    /// Distinguishes this link from a newer one for the same peer, so the old
    /// one closing does not unregister its replacement.
    generation: u64,
    closed: Arc<Notify>,
    /// When this link came up. Anything recorded through it since, the peer
    /// knows about.
    since: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Default)]
pub struct PeerHub {
    links: Arc<Mutex<HashMap<String, Link>>>,
    counter: Arc<AtomicU64>,
    /// Local bot id → peers that have its chat open and want its turns.
    chat_watchers: Arc<Mutex<HashMap<String, HashSet<String>>>>,
}

impl PeerHub {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Link>> {
        self.links.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn is_online(&self, peer_id: &str) -> bool {
        self.lock().contains_key(peer_id)
    }

    /// When the live link to this peer came up.
    pub fn online_since(&self, peer_id: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        self.lock().get(peer_id).map(|link| link.since)
    }

    /// Sends an event frame: no `req_id`, and no answer expected.
    pub fn notify(&self, peer_id: &str, frame: Value) {
        if let Some(link) = self.lock().get(peer_id) {
            let _ = link.out.send(frame);
        }
    }

    /// Remembers that a peer has this bot's chat open.
    pub fn watch_chat(&self, peer_id: &str, bot_id: &str) {
        self.chat_watchers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(bot_id.to_string())
            .or_default()
            .insert(peer_id.to_string());
    }

    /// The online peers watching this bot's chat.
    pub fn chat_watchers(&self, bot_id: &str) -> Vec<String> {
        let watchers = self.chat_watchers.lock().unwrap_or_else(|e| e.into_inner());
        watchers
            .get(bot_id)
            .into_iter()
            .flatten()
            .filter(|peer| self.is_online(peer))
            .cloned()
            .collect()
    }

    /// Drops the peer's link, if any. Used when the peer is revoked.
    pub fn disconnect(&self, peer_id: &str) {
        if let Some(link) = self.lock().remove(peer_id) {
            link.closed.notify_one();
        }
    }

    /// Sends a request and waits for the peer's result.
    pub async fn request(&self, peer_id: &str, mut frame: Value) -> Result<Value, PeerError> {
        let (out, pending) = {
            let links = self.lock();
            let link = links.get(peer_id).ok_or(PeerError::Offline)?;
            (link.out.clone(), link.pending.clone())
        };
        let req_id = self.counter.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock_pending(&pending).insert(req_id, tx);
        frame["req_id"] = json!(req_id);
        if out.send(frame).is_err() {
            lock_pending(&pending).remove(&req_id);
            return Err(PeerError::Offline);
        }
        let response = match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => return Err(PeerError::Rejected("peer link closed".to_string())),
            Err(_) => {
                lock_pending(&pending).remove(&req_id);
                return Err(PeerError::Rejected("peer did not answer".to_string()));
            }
        };
        if response.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(response.get("result").cloned().unwrap_or(Value::Null))
        } else {
            let reason = response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("peer refused the request")
                .to_string();
            match response.get("code").and_then(Value::as_str) {
                Some(code) => Err(PeerError::Refused {
                    code: code.to_string(),
                    reason,
                }),
                None => Err(PeerError::Rejected(reason)),
            }
        }
    }

    /// Runs an authenticated link until either side closes it. Frames from
    /// the peer arrive on `inbound`; frames for it go out on `out`.
    pub(super) async fn serve(
        &self,
        app: Arc<AppState>,
        peer_id: String,
        mut inbound: mpsc::UnboundedReceiver<Value>,
        out: mpsc::UnboundedSender<Value>,
    ) {
        let generation = self.counter.fetch_add(1, Ordering::Relaxed);
        let pending: Pending = Arc::default();
        let closed = Arc::new(Notify::new());
        let replaced = self.lock().insert(
            peer_id.clone(),
            Link {
                out: out.clone(),
                pending: pending.clone(),
                generation,
                closed: closed.clone(),
                since: chrono::Utc::now(),
            },
        );
        if let Some(old) = replaced {
            old.closed.notify_one();
        }
        tracing::info!(peer_id, "peer link up");
        let _ = app.db.touch_peer(&peer_id);
        // Whatever changed in linked projects while the link was down.
        {
            let (app, peer_id) = (app.clone(), peer_id.clone());
            tokio::task::spawn_blocking(move || super::mirror::sync(&app, &peer_id));
        }

        // Events are applied in the order they were sent: a roster, then a
        // newer one, must not land the other way round.
        let (events, ordered) = std::sync::mpsc::channel::<Value>();
        {
            let (app, peer_id) = (app.clone(), peer_id.clone());
            tokio::task::spawn_blocking(move || {
                for frame in ordered {
                    super::inbound::event(&app, &peer_id, &frame);
                }
            });
        }

        loop {
            let frame = tokio::select! {
                frame = inbound.recv() => frame,
                _ = closed.notified() => None,
            };
            let Some(frame) = frame else {
                break;
            };
            if frame.get("type").and_then(Value::as_str) == Some("response") {
                let id = frame.get("req_id").and_then(Value::as_u64);
                if let Some(tx) = id.and_then(|id| lock_pending(&pending).remove(&id)) {
                    let _ = tx.send(frame);
                }
                continue;
            }
            // An event: news from the peer, answered by nothing.
            if frame.get("req_id").is_none() {
                let _ = events.send(frame);
                continue;
            }
            // Handled off the read loop, so one slow request (a result with
            // artifacts to write) does not hold up the responses behind it.
            let app = app.clone();
            let peer_id = peer_id.clone();
            let out = out.clone();
            tokio::task::spawn_blocking(move || {
                let req_id = frame.get("req_id").cloned().unwrap_or(Value::Null);
                let response = match super::inbound::handle(&app, &peer_id, &frame) {
                    Ok(result) => json!({
                        "type": "response", "req_id": req_id, "ok": true, "result": result
                    }),
                    Err(e) => json!({
                        "type": "response", "req_id": req_id, "ok": false,
                        "error": format!("{e:#}"),
                        "code": e.downcast_ref::<super::Refusal>().map(|r| r.code)
                    }),
                };
                let _ = out.send(response);
            });
        }

        let mut links = self.lock();
        if links
            .get(&peer_id)
            .is_some_and(|link| link.generation == generation)
        {
            links.remove(&peer_id);
        }
        drop(links);
        let _ = app.db.touch_peer(&peer_id);
        tracing::info!(peer_id, "peer link down");
    }
}

fn lock_pending(
    pending: &Pending,
) -> std::sync::MutexGuard<'_, HashMap<u64, oneshot::Sender<Value>>> {
    pending.lock().unwrap_or_else(|e| e.into_inner())
}
