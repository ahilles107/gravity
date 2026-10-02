//! The Tasks panel's view of a bot: what it was given, what it delegated,
//! and how each ended.

mod common;

use common::tasks::{drain_until, project_with_bots};
use common::*;
use serde_json::json;

#[tokio::test]
async fn a_bot_sees_its_open_and_finished_tasks_from_both_ends() {
    let (pair, mut clients) = project_with_bots(&["lead", "dev"]).await;
    let sent = clients[0]
        .call(
            "send_message",
            json!({"to": "dev", "kind": "task", "body": "port the updater"}),
        )
        .await;
    let finished = sent["task_id"].as_str().expect("task id").to_string();
    drain_until(&mut clients[1], "port the updater").await;
    clients[1]
        .call(
            "complete_task",
            json!({"task_id": finished, "result": "Ported, tests green."}),
        )
        .await;
    let open = clients[0]
        .call(
            "send_message",
            json!({"to": "dev", "kind": "task", "body": "sign the MSI"}),
        )
        .await["task_id"]
        .as_str()
        .expect("task id")
        .to_string();

    let mut c = WsClient::connect(&pair.d).await;
    let lead = c
        .request(json!({"type": "list_tasks", "bot_id": pair.ids[0]}))
        .await;
    let tasks = lead["tasks"].as_array().expect("tasks");
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["id"], open, "newest first");
    assert_eq!(tasks[0]["state"], "open");
    assert_eq!(tasks[0]["role"], "delegated");
    assert_eq!(tasks[0]["other"]["name"], "dev");
    assert!(tasks[0]["deadline_at"].is_string());
    assert_eq!(tasks[1]["state"], "done");
    assert_eq!(tasks[1]["request"], "port the updater");
    assert_eq!(tasks[1]["result"], "Ported, tests green.");
    assert!(tasks[1]["closed_at"].is_string());

    let dev = c
        .request(json!({"type": "list_tasks", "bot_id": pair.ids[1]}))
        .await;
    assert!(dev["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .all(|t| t["role"] == "assigned" && t["other"]["name"] == "lead"));
}

#[tokio::test]
async fn the_list_previews_long_text_and_get_task_has_all_of_it() {
    let (pair, mut clients) = project_with_bots(&["lead", "dev"]).await;
    let long = "step ".repeat(200);
    let sent = clients[0]
        .call(
            "send_message",
            json!({"to": "dev", "kind": "task", "body": long}),
        )
        .await;
    let task_id = sent["task_id"].as_str().expect("task id").to_string();
    let mut c = WsClient::connect(&pair.d).await;
    let listed = c
        .request(json!({"type": "list_tasks", "bot_id": pair.ids[0]}))
        .await;
    let preview = &listed["tasks"][0];
    assert_eq!(preview["request_truncated"], true);
    assert!(preview["request"].as_str().expect("request").len() < long.len());

    let full = c
        .request(json!({"type": "get_task", "bot_id": pair.ids[0], "task_id": task_id}))
        .await;
    assert_eq!(full["task"]["request"], long);
    assert_eq!(full["task"]["request_truncated"], false);

    let stranger = c
        .request(json!({"type": "get_task", "bot_id": "someone-else", "task_id": task_id}))
        .await;
    assert_eq!(stranger["type"], "error");
}
