//! The owner-chosen order of bots in a project.
//!
//! Order is shared fleet state: one client rearranging the roster must be
//! seen by every other client, survive renames and archival, and never be
//! reshuffled by a new bot arriving.

mod common;

use common::*;
use futures::StreamExt;
use serde_json::{json, Value};

async fn project(c: &mut WsClient, name: &str) -> String {
    let reply = c
        .request(json!({ "type": "create_project", "name": name }))
        .await;
    assert_eq!(reply["type"], "project", "{reply}");
    reply["project"]["id"].as_str().expect("id").to_string()
}

async fn bot(c: &mut WsClient, project_id: &str, name: &str) -> String {
    create_bot(c, project_id, name).await["id"]
        .as_str()
        .expect("bot id")
        .to_string()
}

async fn roster(c: &mut WsClient, project_id: &str) -> Vec<String> {
    let reply = c
        .request(json!({ "type": "list_bots", "project_id": project_id }))
        .await;
    assert_eq!(reply["type"], "bots", "{reply}");
    names(&reply["bots"])
}

fn names(bots: &Value) -> Vec<String> {
    bots.as_array()
        .expect("bots")
        .iter()
        .map(|b| b["name"].as_str().expect("name").to_string())
        .collect()
}

async fn reorder(c: &mut WsClient, project_id: &str, ids: &[&str]) -> Value {
    c.request(json!({ "type": "reorder_bots", "project_id": project_id, "bot_ids": ids }))
        .await
}

#[tokio::test]
async fn new_bots_join_the_end_whatever_their_name() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "Acme").await;
    bot(&mut c, &pid, "Zed").await;
    bot(&mut c, &pid, "Alpha").await;
    let created = create_bot(&mut c, &pid, "Mia").await;

    assert_eq!(roster(&mut c, &pid).await, ["Zed", "Alpha", "Mia"]);
    assert_eq!(created["sort_order"], 2, "{created}");
}

#[tokio::test]
async fn reordering_is_persisted_and_pushed_to_other_clients() {
    let d = spawn_daemon().await;
    let mut owner = WsClient::connect(&d).await;
    let mut watcher = WsClient::connect(&d).await;
    let pid = project(&mut owner, "Acme").await;
    let a = bot(&mut owner, &pid, "Ann").await;
    let b = bot(&mut owner, &pid, "Bob").await;
    let c = bot(&mut owner, &pid, "Cat").await;

    let reply = reorder(&mut owner, &pid, &[&c, &a, &b]).await;
    assert_eq!(reply["type"], "bots", "{reply}");
    assert_eq!(names(&reply["bots"]), ["Cat", "Ann", "Bob"]);

    let push = watcher.wait_for(|v| v["type"] == "bots_reordered").await;
    assert_eq!(push["project_id"], json!(pid));
    assert_eq!(push["bot_ids"], json!([c, a, b]));

    assert_eq!(roster(&mut watcher, &pid).await, ["Cat", "Ann", "Bob"]);
}

#[tokio::test]
async fn unlisted_bots_keep_their_relative_order_after_the_listed_ones() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "Acme").await;
    bot(&mut c, &pid, "Ann").await;
    bot(&mut c, &pid, "Bob").await;
    let cat = bot(&mut c, &pid, "Cat").await;

    let reply = reorder(&mut c, &pid, &[&cat]).await;
    assert_eq!(names(&reply["bots"]), ["Cat", "Ann", "Bob"]);
}

#[tokio::test]
async fn renaming_archiving_and_creating_leave_the_order_alone() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "Acme").await;
    let a = bot(&mut c, &pid, "Ann").await;
    let b = bot(&mut c, &pid, "Bob").await;
    let cat = bot(&mut c, &pid, "Cat").await;
    reorder(&mut c, &pid, &[&cat, &b, &a]).await;

    let renamed = c
        .request(json!({ "type": "update_bot", "bot_id": a, "name": "Aaron" }))
        .await;
    assert_eq!(renamed["type"], "bot", "{renamed}");
    assert_eq!(roster(&mut c, &pid).await, ["Cat", "Bob", "Aaron"]);

    let deleted = c
        .request(json!({ "type": "delete_bot", "bot_id": b }))
        .await;
    assert_eq!(deleted["type"], "ok", "{deleted}");
    assert_eq!(roster(&mut c, &pid).await, ["Cat", "Aaron"]);

    bot(&mut c, &pid, "Abe").await;
    assert_eq!(roster(&mut c, &pid).await, ["Cat", "Aaron", "Abe"]);
}

#[tokio::test]
async fn bad_lists_are_refused_and_change_nothing() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "Acme").await;
    let other = project(&mut c, "Initech").await;
    let a = bot(&mut c, &pid, "Ann").await;
    let b = bot(&mut c, &pid, "Bob").await;
    let stranger = bot(&mut c, &other, "Eve").await;

    let unknown = reorder(&mut c, &pid, &[&b, "nope"]).await;
    assert_eq!(unknown["code"], "not_found", "{unknown}");

    let foreign = reorder(&mut c, &pid, &[&b, &stranger]).await;
    assert_eq!(foreign["code"], "invalid_request", "{foreign}");

    let twice = reorder(&mut c, &pid, &[&b, &a, &b]).await;
    assert_eq!(twice["code"], "invalid_request", "{twice}");

    let missing = reorder(&mut c, "no-such-project", &[&a]).await;
    assert_eq!(missing["code"], "not_found", "{missing}");

    let malformed = c
        .request(json!({ "type": "reorder_bots", "project_id": pid, "bot_ids": "Ann" }))
        .await;
    assert_eq!(malformed["code"], "invalid_request", "{malformed}");

    assert_eq!(roster(&mut c, &pid).await, ["Ann", "Bob"]);
}

#[tokio::test]
async fn reordering_needs_the_control_grant() {
    let d = spawn_daemon().await;
    let mut owner = WsClient::connect(&d).await;
    let pid = project(&mut owner, "Acme").await;
    let a = bot(&mut owner, &pid, "Ann").await;
    let b = bot(&mut owner, &pid, "Bob").await;
    let created = owner
        .request(json!({
            "type": "create_device", "name": "viewer", "capabilities": ["read"]
        }))
        .await;
    let token = token_str(&created).to_string();

    let url = format!("ws://{}/ws", d.addr);
    let (socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("connect");
    let (tx, rx) = socket.split();
    let mut viewer = WsClient {
        tx,
        rx,
        next_req: 1,
    };
    let hello = viewer
        .request(json!({"type": "hello", "protocol_version": 2, "token": token}))
        .await;
    assert_eq!(hello["type"], "hello_ok", "{hello}");

    let denied = reorder(&mut viewer, &pid, &[&b, &a]).await;
    assert_eq!(denied["code"], "forbidden", "{denied}");
    assert_eq!(roster(&mut owner, &pid).await, ["Ann", "Bob"]);
}
