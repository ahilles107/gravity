//! Handshake, user↔bot channel delivery, and lease-free terminal input.

mod common;

use common::*;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message as WsMsg;

#[tokio::test]
async fn handshake_rejects_bad_token() {
    let d = spawn_daemon().await;
    let url = format!("ws://{}/ws", d.addr);
    let (socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("connect");
    let (mut tx, mut rx) = socket.split();
    tx.send(WsMsg::Text(
        json!({"type": "hello", "req_id": "1", "protocol_version": 2, "token": "wrong"})
            .to_string(),
    ))
    .await
    .expect("send");
    let frame = rx.next().await.expect("frame").expect("ok");
    if let WsMsg::Text(text) = frame {
        let v: Value = serde_json::from_str(&text).expect("json");
        assert_eq!(v["type"], "error");
        assert_eq!(v["code"], "auth_failed");
    } else {
        panic!("expected text frame");
    }
}

#[tokio::test]
async fn full_message_flow_user_to_bot() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;

    let project = c
        .request(json!({"type": "create_project", "name": "acme"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let bot = create_bot(&mut c, &project_id, "alice").await;
    let bot_id = bot["id"].as_str().expect("bid").to_string();

    // Provisioned files exist.
    let ws_path = bot["workspace_path"].as_str().expect("ws");
    assert!(std::path::Path::new(ws_path).join("CLAUDE.md").exists());
    assert!(std::path::Path::new(ws_path)
        .parent()
        .expect("root")
        .join("mcp.json")
        .exists());

    // Bots are always-on: creation started it, no request needed.
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;

    // Attach and observe the double runtime banner.
    let attached = c
        .request(json!({"type": "attach", "bot_id": bot_id, "after_seq": 0}))
        .await;
    assert_eq!(attached["type"], "attached");

    // Send a user message over the bus; the delivery worker posts it to the
    // session's inbox socket (never the terminal input).
    let msg = c
        .request(json!({"type": "send_user_message", "to_bot_id": bot_id, "body": "hello bot"}))
        .await;
    assert_eq!(msg["type"], "message");
    let delivered = c
        .wait_for(|v| v["type"] == "delivery_update" && v["delivery"]["state"] == "delivered")
        .await;
    assert_eq!(delivered["delivery"]["bot_id"], json!(bot_id));

    // The double echoes the envelope to the terminal.
    let term = c
        .wait_for(|v| {
            v["type"] == "term" && v["data"].as_str().is_some_and(|s| s.contains("hello bot"))
        })
        .await;
    assert!(term["data"]
        .as_str()
        .expect("data")
        .contains("from USER · chat"));
}

/// A new bot starts into a session with nothing on screen, which reads as
/// broken rather than idle. Creation queues one prompt so the bot opens with an
/// introduction instead of a blank window.
#[tokio::test]
async fn creation_asks_the_new_bot_to_greet_the_user() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let reply = c
        .request(json!({
            "type": "create_bot", "project_id": project_id, "name": "alice"
        }))
        .await;
    assert_eq!(reply["type"], "bot", "{reply}");
    let bot = &reply["bot"];
    let bot_id = bot["id"].as_str().expect("bid").to_string();

    c.request(json!({"type": "attach", "bot_id": bot_id, "after_seq": 0}))
        .await;
    let term = c
        .wait_for(|v| {
            v["type"] == "term"
                && v["data"]
                    .as_str()
                    .is_some_and(|s| s.contains("Greet the user"))
        })
        .await;
    // It reaches the bot as an ordinary note from the daemon, on the same
    // path as any other message.
    assert!(term["data"]
        .as_str()
        .expect("data")
        .contains("from USER \u{b7} note"));
}

/// The conversations that were sent a product tour.
fn toured(d: &TestDaemon) -> Vec<String> {
    d.app
        .db
        .search_messages("\"short tour\"", 10)
        .expect("search")
        .into_iter()
        .map(|m| m.conversation_id)
        .collect()
}

async fn new_project(c: &mut WsClient, name: &str) -> String {
    let project = c
        .request(json!({"type": "create_project", "name": name}))
        .await;
    project["project"]["id"].as_str().expect("pid").to_string()
}

/// Only the project's first bot is asked to tour the product. It is the one a
/// person meets before they know bots can be renamed, chartered or teamed up;
/// repeating that on every later bot is noise to someone who already knows.
#[tokio::test]
async fn only_the_first_bot_of_a_project_tours_the_product() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project_id = new_project(&mut c, "p").await;
    let alice = create_bot(&mut c, &project_id, "alice").await;
    let alice_id = alice["id"].as_str().expect("bid").to_string();
    create_bot(&mut c, &project_id, "bob").await;

    let dm = d
        .app
        .db
        .dm_conversation(&alice_id)
        .expect("dm")
        .expect("conversation");
    assert_eq!(toured(&d), vec![dm.id], "the tour went to the wrong bot");

    // Deleting the only bot does not un-teach the person who saw the tour, so
    // the replacement starts without one.
    c.request(json!({"type": "delete_bot", "bot_id": alice_id}))
        .await;
    create_bot(&mut c, &project_id, "carol").await;
    assert_eq!(toured(&d).len(), 1);

    // A separate project is its own starting point: its first bot introduces
    // the product again, because nothing in it has yet.
    let other_id = new_project(&mut c, "q").await;
    create_bot(&mut c, &other_id, "dave").await;
    assert_eq!(toured(&d).len(), 2);
}

#[tokio::test]
async fn unnamed_bot_creation_allocates_numbered_placeholders() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid");

    let first = c
        .request(json!({"type": "create_bot", "project_id": project_id}))
        .await;
    let second = c
        .request(json!({"type": "create_bot", "project_id": project_id}))
        .await;

    // An explicit null is how a client with no name to give says so.
    let third = c
        .request(json!({"type": "create_bot", "project_id": project_id, "name": null}))
        .await;

    assert_eq!(first["type"], "bot", "{first}");
    assert_eq!(first["bot"]["name"], "New Bot");
    assert_eq!(second["type"], "bot", "{second}");
    assert_eq!(second["bot"]["name"], "New Bot 2");
    assert_eq!(third["type"], "bot", "{third}");
    assert_eq!(third["bot"]["name"], "New Bot 3");
}

/// A malformed name is the caller's mistake, so it comes back as one rather
/// than as an internal failure.
#[tokio::test]
async fn a_non_string_bot_name_is_an_invalid_request() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid");

    let reply = c
        .request(json!({"type": "create_bot", "project_id": project_id, "name": 7}))
        .await;

    assert_eq!(reply["type"], "error", "{reply}");
    assert_eq!(reply["code"], "invalid_request", "{reply}");
}

#[tokio::test]
async fn input_flows_without_lease_and_delivery_defers_while_bot_is_down() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let bot = create_bot(&mut c, &project_id, "bob").await;
    let bot_id = bot["id"].as_str().expect("bid").to_string();
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;
    // Take the session away the way archival does, so nothing restarts it.
    d.app.supervisor.stop_bot(&bot_id).expect("stop");

    // A message sent while the bot has no session stays queued (deferred
    // without burning retry attempts) instead of failing.
    let sent = c
        .request(json!({"type": "send_user_message", "to_bot_id": bot_id, "body": "early bird"}))
        .await;
    let message_id = sent["message"]["id"].clone();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let deliveries = c
        .request(json!({"type": "list_deliveries", "bot_id": bot_id}))
        .await;
    // The bot also has the introduction queued at its creation, delivered
    // while it was still up; this assertion is about the message sent after
    // the session went away.
    let row = deliveries["deliveries"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|r| r["message_id"] == message_id)
        .expect("a delivery for the message just sent");
    assert_eq!(row["state"], "queued", "{deliveries}");
    assert_eq!(row["attempt_count"], 0, "defer must not burn attempts");

    d.app.supervisor.start_bot(&bot_id).expect("start");
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;
    c.request(json!({"type": "attach", "bot_id": bot_id, "after_seq": 0}))
        .await;

    // Once running again, the deferred delivery lands through the inbox socket.
    c.wait_for(|v| v["type"] == "delivery_update" && v["delivery"]["state"] == "delivered")
        .await;
    c.wait_for(|v| {
        v["type"] == "term" && v["data"].as_str().is_some_and(|s| s.contains("early bird"))
    })
    .await;

    // Typing needs no lease: two clients can both send input.
    c.send(json!({"type": "input", "bot_id": bot_id, "data": "typed-by-c1!"}))
        .await;
    c.wait_for(|v| {
        v["type"] == "term"
            && v["data"]
                .as_str()
                .is_some_and(|s| s.contains("typed-by-c1!"))
    })
    .await;
    let mut c2 = WsClient::connect(&d).await;
    c2.send(json!({"type": "input", "bot_id": bot_id, "data": "typed-by-c2!"}))
        .await;
    c.wait_for(|v| {
        v["type"] == "term"
            && v["data"]
                .as_str()
                .is_some_and(|s| s.contains("typed-by-c2!"))
    })
    .await;
}

/// A forced resize at an unchanged size is the client's repaint nudge after a
/// replay that could not rebuild the screen. The runtime must observe a real
/// shrink and then the restore — two sizes, in order — because an instant
/// restore would coalesce into one SIGWINCH the runtime reads as "nothing
/// changed" and never repaint.
#[tokio::test]
async fn a_forced_resize_at_the_same_size_still_reaches_the_runtime() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let bot = create_bot(&mut c, &project_id, "alice").await;
    let bot_id = bot["id"].as_str().expect("bid").to_string();
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;
    c.request(json!({"type": "attach", "bot_id": bot_id, "after_seq": 0}))
        .await;

    // Establish the size the daemon considers current.
    c.send(json!({"type": "resize", "bot_id": bot_id, "cols": 100, "rows": 40}))
        .await;
    c.wait_for(|v| {
        v["type"] == "term"
            && v["data"]
                .as_str()
                .is_some_and(|s| s.contains("[resize 100x40]"))
    })
    .await;

    // A repeated nudge replaces the first pending restore. Both shrink calls
    // reach the runtime, followed by the one coalesced restore.
    c.send(json!({"type": "resize", "bot_id": bot_id, "cols": 100, "rows": 40, "force": true}))
        .await;
    c.send(json!({"type": "resize", "bot_id": bot_id, "cols": 100, "rows": 40, "force": true}))
        .await;
    // Output may merge both shrink markers and the restore into one push.
    let mut output = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !output.contains("[resize 100x40]") {
        let frame = tokio::time::timeout_at(
            deadline,
            c.wait_for(|v| v["type"] == "term" && v["bot_id"] == bot_id),
        )
        .await
        .expect("timed out waiting for resize output");
        output.push_str(frame["data"].as_str().expect("terminal data"));
    }
    let (before_restore, _) = output.split_once("[resize 100x40]").expect("restore");
    assert_eq!(
        before_restore.matches("[resize 100x39]").count(),
        2,
        "{output}"
    );
}
