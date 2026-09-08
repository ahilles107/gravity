//! Shared harness for the end-to-end tests: a real daemon instance backed by
//! the deterministic runtime double, plus a small WebSocket client.
//!
//! Each integration binary compiles the whole harness but uses only part of it.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use gravityd::app::AppState;
use gravityd::config::{Config, RuntimeKind};
use gravityd::db::Db;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message as WsMsg;

pub mod tasks;

pub struct TestDaemon {
    pub app: Arc<AppState>,
    pub addr: SocketAddr,
    pub _home: tempfile::TempDir,
}

pub async fn spawn_daemon() -> TestDaemon {
    spawn_daemon_with(|_| {}).await
}

/// Spawn a daemon with the config tweaked before anything reads it — the
/// supervisor keeps its own clone, so limits must be set up front.
pub async fn spawn_daemon_with(tweak: impl FnOnce(&mut Config)) -> TestDaemon {
    let home = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config {
        home: home.path().to_path_buf(),
        runtime: RuntimeKind::Double,
        ..Config::default()
    };
    cfg.delivery.poll_interval_ms = 50;
    cfg.scheduler.tick_interval_ms = 100;
    tweak(&mut cfg);

    let db = Db::open(&cfg.db_path()).expect("db");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    cfg.port = addr.port();
    let app = AppState::new(cfg, db).expect("app");
    gravityd::server::spawn_workers(&app);
    let router = gravityd::server::router(app.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    TestDaemon {
        app,
        addr,
        _home: home,
    }
}

pub struct WsClient {
    pub tx: futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        WsMsg,
    >,
    pub rx: futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    pub next_req: u64,
}

impl WsClient {
    pub async fn connect(d: &TestDaemon) -> Self {
        let url = format!("ws://{}/ws", d.addr);
        let (socket, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("ws connect");
        let (tx, rx) = socket.split();
        let mut c = Self {
            tx,
            rx,
            next_req: 1,
        };
        let reply = c
            .request(json!({
                "type": "hello",
                "protocol_version": 2,
                "token": d.app.secrets.client_token(),
                "client": "test/0"
            }))
            .await;
        assert_eq!(reply["type"], "hello_ok", "handshake failed: {reply}");
        c
    }

    pub async fn send(&mut self, mut req: Value) -> String {
        let req_id = self.next_req.to_string();
        self.next_req += 1;
        req["req_id"] = json!(req_id);
        self.tx
            .send(WsMsg::Text(req.to_string()))
            .await
            .expect("ws send");
        req_id
    }

    /// Send a request and wait for the frame carrying its req_id, buffering
    /// nothing (pushes are skipped).
    pub async fn request(&mut self, req: Value) -> Value {
        let req_id = self.send(req).await;
        self.wait_for(|v| v["req_id"] == json!(req_id.clone()))
            .await
    }

    pub async fn wait_for(&mut self, pred: impl Fn(&Value) -> bool) -> Value {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let frame = tokio::time::timeout_at(deadline, self.rx.next())
                .await
                .expect("timed out waiting for frame")
                .expect("stream ended")
                .expect("ws error");
            if let WsMsg::Text(text) = frame {
                let v: Value = serde_json::from_str(&text).expect("json frame");
                if pred(&v) {
                    return v;
                }
            }
        }
    }
}

pub async fn create_bot(c: &mut WsClient, project_id: &str, name: &str) -> Value {
    let bot = c
        .request(json!({
            "type": "create_bot", "project_id": project_id, "name": name,
            "description": format!("{name} bot"), "instructions": "be helpful"
        }))
        .await;
    assert_eq!(bot["type"], "bot", "{bot}");
    bot["bot"].clone()
}

pub async fn raw_hello(d: &TestDaemon, token: &str) -> Value {
    let url = format!("ws://{}/ws", d.addr);
    let (socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("connect");
    let (mut tx, mut rx) = socket.split();
    tx.send(WsMsg::Text(
        json!({"type": "hello", "req_id": "1", "protocol_version": 2, "token": token}).to_string(),
    ))
    .await
    .expect("send hello");
    loop {
        let frame = rx.next().await.expect("frame").expect("ws ok");
        if let WsMsg::Text(text) = frame {
            return serde_json::from_str(&text).expect("json");
        }
    }
}

pub fn token_str(created: &Value) -> &str {
    created["token"].as_str().expect("token")
}

/// Minimal MCP client speaking the daemon's JSON-RPC subset as one bot.
///
/// Identity comes from the bearer token, exactly as it does for a real bot, so
/// these tests exercise the same authorisation path production does.
pub struct McpClient {
    url: String,
    token: String,
    http: reqwest::Client,
    next_id: u64,
}

impl McpClient {
    pub fn new(d: &TestDaemon, token: &str) -> Self {
        Self {
            url: format!("http://{}/mcp", d.addr),
            token: token.to_string(),
            http: reqwest::Client::new(),
            next_id: 1,
        }
    }

    /// Call a tool and return its decoded JSON payload, asserting success.
    pub async fn call(&mut self, name: &str, args: Value) -> Value {
        let raw = self.call_raw(name, args).await;
        assert_ne!(raw["isError"], json!(true), "tool {name} failed: {raw}");
        let text = raw["content"][0]["text"].as_str().expect("text content");
        serde_json::from_str(text).expect("tool payload is json")
    }

    /// Call a tool and return the raw MCP result, errors included.
    pub async fn call_raw(&mut self, name: &str, args: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let body = json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args }
        });
        let reply: Value = self
            .http
            .post(&self.url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .expect("mcp request")
            .json()
            .await
            .expect("mcp json");
        reply["result"].clone()
    }

    /// The advertised tool list.
    pub async fn tools(&mut self) -> Value {
        let body = json!({ "jsonrpc": "2.0", "id": 99, "method": "tools/list" });
        let reply: Value = self
            .http
            .post(&self.url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .expect("mcp request")
            .json()
            .await
            .expect("mcp json");
        reply["result"].clone()
    }

    /// Whether this client's token still authenticates at all.
    pub async fn is_authorized(&self) -> bool {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" });
        let status = self
            .http
            .post(&self.url)
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .expect("mcp request")
            .status();
        status.is_success()
    }
}

/// Everything the bot's terminal has shown so far. The runtime double opens
/// with the flags it was spawned with, so this is also how a test sees what
/// the daemon asked the runtime for.
pub fn terminal(d: &TestDaemon, bot_id: &str) -> String {
    let term = d.app.supervisor.term(bot_id).expect("terminal buffer");
    let bytes: Vec<u8> = term
        .replay_after(0)
        .frames
        .into_iter()
        .flat_map(|f| f.data)
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}
