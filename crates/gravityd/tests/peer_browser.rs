//! A linked bot's browser from the other machine: the Mac watches the
//! Windows bot's browser through the peer link, viewers there share one feed,
//! and its browser log is read from its machine.

mod common;

use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use common::devtools::{fake_devtools, start_browser};
use common::peers::{team, wait_until};
use common::*;
use serde_json::{json, Value};

#[tokio::test]
async fn the_other_machines_bot_browser_can_be_watched() {
    let mut t = team().await;
    let windev = t.linked_windev.clone();
    let port = fake_devtools("Windows build", Arc::new(AtomicUsize::new(0))).await;
    start_browser(&t.win, &t.windev_id, port);

    let ok = t
        .mac_client
        .request(json!({"type": "watch_browser", "bot_id": windev}))
        .await;
    assert_eq!(ok["type"], "ok", "{ok}");
    let tabs = t
        .mac_client
        .wait_for(|v| v["type"] == "browser_tabs" && v["open"] == true)
        .await;
    // Named for the stand-in here, not the bot's id on its machine.
    assert_eq!(tabs["bot_id"], windev.as_str());
    assert_eq!(tabs["tabs"][0]["title"], "Windows build");
    let frame = t
        .mac_client
        .wait_for(|v| v["type"] == "browser_frame")
        .await;
    assert_eq!(frame["bot_id"], windev.as_str());
    assert_eq!(frame["data"], "SlBFRw==");

    // A phone on the Mac shares the feed: the PC watches once.
    let mut phone = WsClient::connect(&t.mac).await;
    phone
        .request(json!({"type": "watch_browser", "bot_id": windev}))
        .await;
    let shared = phone.wait_for(|v| v["type"] == "browser_frame").await;
    assert_eq!(shared["bot_id"], windev.as_str());
    assert_eq!(t.win.app.peers.browsers.serving(), 1);

    t.mac_client
        .request(json!({"type": "unwatch_browser"}))
        .await;
    assert_eq!(t.win.app.peers.browsers.serving(), 1);
    phone.request(json!({"type": "unwatch_browser"})).await;
    let win = &t.win;
    wait_until("the PC stops watching", || {
        win.app.peers.browsers.serving() == 0
    })
    .await;
}

#[tokio::test]
async fn the_other_machines_bot_browser_log_is_read_from_there() {
    let mut t = team().await;
    let windev = t
        .win
        .app
        .db
        .get_bot(&t.windev_id)
        .expect("db")
        .expect("bot");
    let mangled: String = windev
        .workspace_path
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let dir = t
        .win
        .app
        .cfg
        .user_home
        .join(".claude/projects")
        .join(mangled);
    std::fs::create_dir_all(&dir).expect("dir");
    let records: Vec<String> = [
        json!({"type": "user", "uuid": "turn-1", "timestamp": "2026-10-01T10:00:00Z",
               "isMeta": true, "origin": {"kind": "peer"},
               "message": {"content":
                   "Another Claude session sent a message:\n[msg #7 from USER · chat] check the store page"}}),
        json!({"type": "assistant", "uuid": "a1", "timestamp": "2026-10-01T10:00:02Z",
               "message": {"content": [
                   {"type": "tool_use", "id": "nav", "name": "mcp__playwright__browser_navigate",
                    "input": {"url": "https://store.example.com/"}}]}}),
    ]
    .iter()
    .map(Value::to_string)
    .collect();
    std::fs::write(dir.join("session.jsonl"), records.join("\n") + "\n").expect("write");

    let log = t
        .mac_client
        .request(json!({"type": "list_browser_activity", "bot_id": t.linked_windev}))
        .await;
    assert_eq!(log["type"], "browser_activity", "{log}");
    assert_eq!(log["bot_id"], t.linked_windev.as_str());
    assert_eq!(log["activity"][0]["title"], "Opened store.example.com");
}
