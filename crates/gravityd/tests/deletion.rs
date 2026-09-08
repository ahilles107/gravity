//! Deletion, the population cap, and churn.
//!
//! Deleting a bot used to leave four things behind: the `bot` row itself, a
//! working credential, stranded tasks, and the DM conversation. Each has a
//! named regression test here, because bots now call `delete_bot` themselves
//! and nothing prompts a human first.

mod common;

use common::{spawn_daemon, spawn_daemon_with, McpClient, TestDaemon, WsClient};
use serde_json::{json, Value};

async fn project(c: &mut WsClient, name: &str) -> String {
    let reply = c
        .request(json!({ "type": "create_project", "name": name }))
        .await;
    reply["project"]["id"]
        .as_str()
        .expect("project id")
        .to_string()
}

async fn bot_with_bus(
    d: &TestDaemon,
    c: &mut WsClient,
    pid: &str,
    name: &str,
) -> (Value, McpClient) {
    let bot = common::create_bot(c, pid, name).await;
    let id = bot["id"].as_str().expect("bot id").to_string();
    let token = d.app.secrets.bot_token(&id).expect("token");
    (bot, McpClient::new(d, &token))
}

/// Regression: the daemon had no `DELETE FROM bot` anywhere, so a deleted bot
/// stayed listed and addressable.
#[tokio::test]
async fn a_deleted_bot_leaves_the_roster() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (lead, mut bus) = bot_with_bus(&d, &mut c, &pid, "Lead").await;

    bus.call(
        "create_bot",
        json!({ "name": "Helper", "description": "d", "instructions": "i" }),
    )
    .await;
    bus.call("delete_bot", json!({ "name": "Helper" })).await;

    assert!(
        d.app
            .db
            .get_bot_by_name(&pid, "Helper")
            .expect("q")
            .is_none(),
        "deleted bot must not resolve by name"
    );
    let listed = bus.call("list_bots", json!({})).await;
    let names: Vec<&str> = listed["bots"]
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|b| b["name"].as_str())
        .collect();
    assert!(!names.contains(&"Helper"), "still listed: {names:?}");

    // The creator survives, and the message history still resolves its sender.
    assert!(d
        .app
        .db
        .get_live_bot(lead["id"].as_str().expect("id"))
        .expect("q")
        .is_some());
}

/// Regression: `Secrets` had no `remove_bot_token`, so a deleted bot's
/// credential kept authenticating `/mcp` and `/hook` forever.
#[tokio::test]
async fn a_deleted_bots_token_stops_working() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (_lead, mut bus) = bot_with_bus(&d, &mut c, &pid, "Lead").await;

    bus.call(
        "create_bot",
        json!({ "name": "Helper", "description": "d", "instructions": "i" }),
    )
    .await;
    let helper = d
        .app
        .db
        .get_bot_by_name(&pid, "Helper")
        .expect("q")
        .expect("helper");
    let helper_bus = McpClient::new(&d, &d.app.secrets.bot_token(&helper.id).expect("token"));
    assert!(
        helper_bus.is_authorized().await,
        "token should work before deletion"
    );

    bus.call("delete_bot", json!({ "name": "Helper" })).await;

    assert!(
        !helper_bus.is_authorized().await,
        "a deleted bot's token must stop authenticating"
    );
}

/// Regression: open tasks were deleted silently, so the requesting bot waited
/// forever for a `complete_task` that could never arrive.
#[tokio::test]
async fn deleting_an_assignee_releases_whoever_was_waiting() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (_lead, mut bus) = bot_with_bus(&d, &mut c, &pid, "Lead").await;
    // The cancellation notice arrives as a real bus delivery, so wait for the
    // requester's always-on session to come up.
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;

    bus.call(
        "create_bot",
        json!({ "name": "Helper", "description": "d", "instructions": "i" }),
    )
    .await;
    // Lead delegates, creating an open task assigned to Helper.
    bus.call(
        "send_message",
        json!({ "to": "Helper", "body": "please do the thing", "kind": "task" }),
    )
    .await;
    let helper = d
        .app
        .db
        .get_bot_by_name(&pid, "Helper")
        .expect("q")
        .expect("helper");
    assert_eq!(
        d.app.db.open_tasks_for(&helper.id).expect("tasks").len(),
        1,
        "expected one open task"
    );

    bus.call("delete_bot", json!({ "name": "Helper" })).await;

    // The task is cancelled, not vanished...
    assert!(
        d.app
            .db
            .open_tasks_for(&helper.id)
            .expect("tasks")
            .is_empty(),
        "task should no longer be open"
    );
    // ...and the requester was actually told, rather than left hanging. The
    // notice rides the normal delivery worker, so poll rather than assuming it
    // has already landed.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut seen: Vec<String> = Vec::new();
    loop {
        let inbox = bus.call("check_inbox", json!({})).await;
        seen.extend(
            inbox["messages"]
                .as_array()
                .expect("array")
                .iter()
                .filter_map(|m| m["body"].as_str())
                .map(str::to_string),
        );
        if seen.iter().any(|b| b.contains("Task cancelled")) {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "requester was never notified; saw: {seen:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// The workspace is deliberately kept: deleting a bot must not destroy work it
/// produced.
#[tokio::test]
async fn deletion_keeps_the_workspace_and_the_history() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (_lead, mut bus) = bot_with_bus(&d, &mut c, &pid, "Lead").await;

    bus.call(
        "create_bot",
        json!({ "name": "Helper", "description": "d", "instructions": "i" }),
    )
    .await;
    let helper = d
        .app
        .db
        .get_bot_by_name(&pid, "Helper")
        .expect("q")
        .expect("helper");
    let artifact = std::path::Path::new(&helper.workspace_path).join("result.txt");
    std::fs::write(&artifact, "hard-won output").expect("write");

    bus.call("delete_bot", json!({ "name": "Helper" })).await;

    assert!(artifact.exists(), "deleting a bot must not delete its work");
    let archived = d
        .app
        .db
        .get_bot(&helper.id)
        .expect("q")
        .expect("row survives");
    assert!(archived.deleted_at.is_some());
}

/// A deleted bot frees its name immediately, so create → delete → create works.
#[tokio::test]
async fn a_deleted_name_can_be_reused() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (_lead, mut bus) = bot_with_bus(&d, &mut c, &pid, "Lead").await;

    for _ in 0..3 {
        let created = bus
            .call(
                "create_bot",
                json!({ "name": "Helper", "description": "d", "instructions": "i" }),
            )
            .await;
        assert_eq!(created["name"], "Helper");
        bus.call("delete_bot", json!({ "name": "Helper" })).await;
    }

    // Each incarnation got its own directory rather than inheriting the last
    // one's files.
    let all = d.app.db.list_bots_with_archived(Some(&pid)).expect("list");
    let helpers: Vec<&str> = all
        .iter()
        .filter(|b| b.name.starts_with("Helper"))
        .map(|b| b.dir_name.as_str())
        .collect();
    assert_eq!(helpers.len(), 3);
    let mut unique = helpers.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique.len(),
        3,
        "workspaces must not be shared: {helpers:?}"
    );
}

/// A bot cannot delete itself: the session executing the call would be torn
/// down underneath it.
#[tokio::test]
async fn a_bot_cannot_delete_itself() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (_lead, mut bus) = bot_with_bus(&d, &mut c, &pid, "Lead").await;

    let err = bus.call_raw("delete_bot", json!({ "name": "Lead" })).await;
    assert_eq!(err["isError"], true, "{err}");
    assert!(d.app.db.get_bot_by_name(&pid, "Lead").expect("q").is_some());
}

/// The population cap is the only limit on creation, so it has to hold exactly.
#[tokio::test]
async fn the_population_cap_holds_at_the_boundary() {
    let d = spawn_daemon_with(|c| c.max_bots_per_project = 4).await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (_lead, mut bus) = bot_with_bus(&d, &mut c, &pid, "Lead").await;

    // Lead itself counts, so three more reach the cap of four.
    for n in 1..=3 {
        let created = bus
            .call(
                "create_bot",
                json!({ "name": format!("Helper{n}"), "description": "d", "instructions": "i" }),
            )
            .await;
        assert_eq!(created["name"], format!("Helper{n}"));
    }
    assert_eq!(d.app.db.count_live_bots(&pid).expect("count"), 4);

    let err = bus
        .call_raw(
            "create_bot",
            json!({ "name": "OneTooMany", "description": "d", "instructions": "i" }),
        )
        .await;
    assert_eq!(err["isError"], true, "{err}");
    let text = err["content"][0]["text"].as_str().expect("text");
    assert!(
        text.contains('4') && text.to_lowercase().contains("limit"),
        "error should name the limit and the count: {text}"
    );
    assert_eq!(d.app.db.count_live_bots(&pid).expect("count"), 4);
}

/// Recursion is bounded by population alone — there is no spawn-depth limit,
/// so this is the direct evidence that none is needed. Each generation creates
/// the next until the project fills up.
#[tokio::test]
async fn recursive_creation_converges_on_the_cap() {
    let d = spawn_daemon_with(|c| c.max_bots_per_project = 5).await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (root, _root_bus) = bot_with_bus(&d, &mut c, &pid, "Gen0").await;

    let mut current = root["id"].as_str().expect("id").to_string();
    let mut generations = 0;
    for n in 1..20 {
        let token = d.app.secrets.bot_token(&current).expect("token");
        let mut bus = McpClient::new(&d, &token);
        let reply = bus
            .call_raw(
                "create_bot",
                json!({ "name": format!("Gen{n}"), "description": "d", "instructions": "i" }),
            )
            .await;
        if reply["isError"] == json!(true) {
            break;
        }
        let child = d
            .app
            .db
            .get_bot_by_name(&pid, &format!("Gen{n}"))
            .expect("q")
            .expect("child");
        current = child.id;
        generations += 1;
    }

    assert!(
        generations >= 4,
        "expected the chain to reach the cap, got {generations}"
    );
    assert_eq!(
        d.app.db.count_live_bots(&pid).expect("count"),
        5,
        "population must stop exactly at the cap however deeply bots nest"
    );
}

/// Churn is the accepted cost of letting bots delete their own children:
/// create → delete → create never exceeds the cap and always frees its slot.
#[tokio::test]
async fn create_delete_churn_stays_bounded() {
    let d = spawn_daemon_with(|c| c.max_bots_per_project = 3).await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (_lead, mut bus) = bot_with_bus(&d, &mut c, &pid, "Lead").await;

    for n in 0..6 {
        bus.call(
            "create_bot",
            json!({ "name": format!("Worker{n}"), "description": "d", "instructions": "i" }),
        )
        .await;
        assert!(
            d.app.db.count_live_bots(&pid).expect("count") <= 3,
            "cap breached on iteration {n}"
        );
        bus.call("delete_bot", json!({ "name": format!("Worker{n}") }))
            .await;
    }

    assert_eq!(
        d.app.db.count_live_bots(&pid).expect("count"),
        1,
        "only Lead should remain live"
    );
    // Every cycle is on the record, so silent churn is still auditable.
    let lead_id = _lead["id"].as_str().expect("id");
    let all_archived = d.app.db.list_bots_with_archived(Some(&pid)).expect("list");
    assert_eq!(all_archived.len(), 7, "6 workers + lead");
    assert!(
        !d.app
            .db
            .list_bot_revisions(lead_id, 50)
            .expect("revs")
            .is_empty(),
        "lead should have at least its creation recorded"
    );
}
