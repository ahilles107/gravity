//! MCP bot-to-bot tasks and crash recovery.

mod common;

use std::time::Duration;

use common::*;
use gravityd::app::AppState;
use gravityd::config::{Config, RuntimeKind};
use gravityd::db::Db;
use serde_json::{json, Value};

/// Inbox messages from other bots, dropping the daemon's introduction prompt.
///
/// Every bot is asked to introduce itself when it is created, so a fresh bot's
/// inbox holds one note from `system` before any test traffic reaches it.
fn peer_messages(inbox: &Value) -> Vec<Value> {
    inbox["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .filter(|m| m["from"] != json!("system"))
        .cloned()
        .collect()
}

#[tokio::test]
async fn mcp_bot_to_bot_task_and_completion() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let alice = create_bot(&mut c, &project_id, "alice").await;
    let bob = create_bot(&mut c, &project_id, "bob").await;
    let alice_id = alice["id"].as_str().expect("id").to_string();
    let bob_id = bob["id"].as_str().expect("id").to_string();

    let http = reqwest::Client::new();
    let mcp_url = format!("http://{}/mcp", d.addr);
    let alice_token = d.app.secrets.bot_token(&alice_id).expect("token");
    let bob_token = d.app.secrets.bot_token(&bob_id).expect("token");

    let call = |token: String, name: &str, args: Value| {
        let http = http.clone();
        let url = mcp_url.clone();
        let name = name.to_string();
        async move {
            let res: Value = http
                .post(&url)
                .bearer_auth(token)
                .json(&json!({
                    "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                    "params": { "name": name, "arguments": args }
                }))
                .send()
                .await
                .expect("mcp call")
                .json()
                .await
                .expect("mcp json");
            let text = res["result"]["content"][0]["text"]
                .as_str()
                .expect("content text")
                .to_string();
            assert_ne!(res["result"]["isError"], json!(true), "tool error: {text}");
            serde_json::from_str::<Value>(&text).unwrap_or(json!({ "raw": text }))
        }
    };

    // Unauthenticated MCP is rejected.
    let unauth = http
        .post(&mcp_url)
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
        .send()
        .await
        .expect("send");
    assert_eq!(unauth.status(), 401);

    // Alice delegates to Bob.
    let sent = call(
        alice_token.clone(),
        "send_message",
        json!({"to": "bob", "body": "Please review PR #1", "kind": "task"}),
    )
    .await;
    let task_id = sent["task_id"].as_str().expect("task id").to_string();

    // Duplicate transport delivery of the same send is deduplicated at the
    // application level: bob's inbox sees the message once.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let inbox = call(bob_token.clone(), "check_inbox", json!({})).await;
    let msgs = peer_messages(&inbox);
    assert_eq!(msgs.len(), 1, "expected exactly one inbox message: {inbox}");
    assert_eq!(msgs[0]["from"], "alice");
    assert_eq!(msgs[0]["kind"], "task");
    // The assignee is told which task the message opened; without it Bob has
    // no id to call complete_task with.
    assert_eq!(msgs[0]["task_id"], json!(task_id), "{inbox}");

    // Bob may answer the requester mid-task: a reply is not a delegation, so
    // the origin-chain loop guard lets it through and opens no new task.
    let reply = call(
        bob_token.clone(),
        "send_message",
        json!({"to": "alice", "body": "Which branch?", "kind": "reply"}),
    )
    .await;
    assert!(reply["task_id"].is_null(), "reply opened a task: {reply}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let alice_inbox = call(alice_token.clone(), "check_inbox", json!({})).await;
    let msgs = peer_messages(&alice_inbox);
    assert_eq!(msgs.len(), 1, "{alice_inbox}");
    assert_eq!(msgs[0]["kind"], "reply");
    assert!(msgs[0]["task_id"].is_null(), "{alice_inbox}");

    // Handing work back up the chain is still refused as a loop.
    let looped: Value = http
        .post(&mcp_url)
        .bearer_auth(&bob_token)
        .json(&json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "send_message",
                        "arguments": {"to": "alice", "body": "you do it", "kind": "task"} }
        }))
        .send()
        .await
        .expect("send")
        .json()
        .await
        .expect("json");
    assert_eq!(looped["result"]["isError"], json!(true), "{looped}");

    // Bob completes; Alice receives the correlated done message.
    call(
        bob_token.clone(),
        "complete_task",
        json!({"task_id": task_id, "result": "Reviewed. Two blockers."}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let alice_inbox = call(alice_token.clone(), "check_inbox", json!({})).await;
    let msgs = peer_messages(&alice_inbox);
    assert_eq!(msgs.len(), 1, "{alice_inbox}");
    assert_eq!(msgs[0]["kind"], "done");
    assert!(msgs[0]["body"].as_str().expect("body").contains("blockers"));

    // Completing twice fails.
    let res: Value = http
        .post(&mcp_url)
        .bearer_auth(&bob_token)
        .json(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "complete_task",
                        "arguments": {"task_id": task_id, "result": "again"} }
        }))
        .send()
        .await
        .expect("send")
        .json()
        .await
        .expect("json");
    assert_eq!(res["result"]["isError"], json!(true));
}

#[tokio::test]
async fn bus_survives_daemon_restart_with_pending_delivery() {
    let home = tempfile::tempdir().expect("tempdir");
    let mut cfg = Config {
        home: home.path().to_path_buf(),
        runtime: RuntimeKind::Double,
        ..Config::default()
    };
    // Slow worker so the delivery is still queued when we "crash".
    cfg.delivery.poll_interval_ms = 60_000;

    let (_bot_id, delivery_id) = {
        let db = Db::open(&cfg.db_path()).expect("db");
        let app = AppState::new(cfg.clone(), db).expect("app");
        let project = app.db.create_project("p", "p").expect("project");
        let bot = app
            .db
            .create_bot(&project.id, "alice", "", "", "", "/tmp/x", "alice", None)
            .expect("bot");
        let conv = app.db.dm_conversation(&bot.id).expect("conv").expect("dm");
        let msg = app
            .db
            .insert_message(
                &conv.id,
                &gravityd::messaging::user_sender(),
                bus::MessageKind::Task,
                "durable?",
                None,
                None,
            )
            .expect("msg");
        let delivery = app
            .db
            .enqueue_delivery(&msg.id, &bot.id, "restart-key")
            .expect("delivery");
        (bot.id.clone(), delivery.id.clone())
        // App dropped here: simulated daemon exit after persist, before delivery.
    };

    // Fresh daemon over the same database.
    cfg.delivery.poll_interval_ms = 50;
    let db = Db::open(&cfg.db_path()).expect("reopen db");
    let app = AppState::new(cfg, db).expect("app2");
    // No one starts the bot: booting the daemon reconciles it into a session.
    gravityd::server::spawn_workers(&app);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let d = app
            .db
            .get_delivery(&delivery_id)
            .expect("get")
            .expect("row");
        if d.state == bus::DeliveryState::Delivered {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "delivery never completed after restart: {:?}",
            d.state
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
