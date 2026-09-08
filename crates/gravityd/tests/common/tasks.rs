//! Task-shaped helpers for the bus guardrail tests: a project of bots, each
//! with its own MCP client.

use serde_json::{json, Value};

use super::{create_bot, spawn_daemon, McpClient, TestDaemon, WsClient};

/// Inbox messages from other bots, dropping the daemon's introduction prompt.
pub fn peer_messages(inbox: &Value) -> Vec<Value> {
    inbox["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .filter(|m| m["from"] != json!("system"))
        .cloned()
        .collect()
}

pub struct Pair {
    pub d: TestDaemon,
    pub ids: Vec<String>,
}

/// A project with the named bots, plus an MCP client per bot.
pub async fn project_with_bots(names: &[&str]) -> (Pair, Vec<McpClient>) {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let mut ids = Vec::new();
    let mut clients = Vec::new();
    for name in names {
        let bot = create_bot(&mut c, &project_id, name).await;
        let id = bot["id"].as_str().expect("id").to_string();
        let token = d.app.secrets.bot_token(&id).expect("token");
        clients.push(McpClient::new(&d, &token));
        ids.push(id);
    }
    (Pair { d, ids }, clients)
}

pub fn error_text(raw: &Value) -> &str {
    assert_eq!(raw["isError"], json!(true), "expected a refusal: {raw}");
    raw["content"][0]["text"].as_str().expect("text")
}
