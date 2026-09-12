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

/// Two projects on one daemon, each with one bot — what the cross-project
/// rules need, since a bot's project comes from its bearer token.
pub async fn two_projects_one_bot_each() -> (Pair, Vec<McpClient>) {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let mut ids = Vec::new();
    let mut clients = Vec::new();
    for (project, bot) in [("alpha", "ann"), ("beta", "bo")] {
        let created = c
            .request(json!({"type": "create_project", "name": project}))
            .await;
        let project_id = created["project"]["id"].as_str().expect("pid").to_string();
        let created = create_bot(&mut c, &project_id, bot).await;
        let id = created["id"].as_str().expect("id").to_string();
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

/// Drain a bot's inbox until `needle` shows up in a message body, or give up.
///
/// Delivery is asynchronous and `check_inbox` consumes what it returns, so a
/// single read races the delivery worker and a naive retry loses whatever the
/// last read already took. This accumulates instead.
pub async fn drain_until(client: &mut McpClient, needle: &str) -> Vec<Value> {
    let mut seen = Vec::new();
    for _ in 0..40 {
        let inbox = client.call("check_inbox", json!({})).await;
        seen.extend(peer_messages(&inbox));
        if seen
            .iter()
            .any(|m| m["body"].as_str().is_some_and(|b| b.contains(needle)))
        {
            return seen;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    seen
}

/// Drain a bot's inbox for a fixed settling period, returning everything that
/// arrived. For asserting that something is delivered *once*, where waiting
/// for a hit would stop before a duplicate could show up.
pub async fn drain_all(client: &mut McpClient) -> Vec<Value> {
    let mut seen = Vec::new();
    for _ in 0..12 {
        let inbox = client.call("check_inbox", json!({})).await;
        seen.extend(peer_messages(&inbox));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    seen
}
