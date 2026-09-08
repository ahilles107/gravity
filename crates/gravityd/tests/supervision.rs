//! Always-on supervision: nothing starts a bot by hand, and a session that
//! goes away comes back on its own.

mod common;

use common::*;
use serde_json::{json, Value};

/// A session that exits unasked is a crash, and the supervision loop restarts
/// it. There is no Start button to press.
#[tokio::test]
async fn a_crashed_bot_is_restarted_by_the_supervision_loop() {
    let d = spawn_daemon_with(|cfg| cfg.supervision_interval_ms = 100).await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let bot = create_bot(&mut c, &project_id, "alice").await;
    let bot_id = bot["id"].as_str().expect("bid").to_string();
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;

    // Ctrl-D ends the double runtime. The daemon did not ask for it, so this
    // is the unsolicited-exit path.
    c.send(json!({"type": "input", "bot_id": bot_id, "data": "\u{4}"}))
        .await;
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "crashed")
        .await;
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;
}

/// Every live bot in the database is running shortly after the daemon boots,
/// with no client connected at all.
#[tokio::test]
async fn every_live_bot_starts_when_the_daemon_boots() {
    let d = spawn_daemon_with(|cfg| cfg.supervision_interval_ms = 100).await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    for name in ["alice", "bob", "carol"] {
        create_bot(&mut c, &project_id, name).await;
    }

    // The per-project cap is gone: everything the project holds is running.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while d.app.supervisor.active_total() < 3 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "only {} bots running",
            d.app.supervisor.active_total()
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// Claude Code's `Notification` hook fires both for permission prompts and for
/// a prompt that has sat idle. Only the first is an approval stop; the idle
/// ping used to strand a finished bot on "waiting for approval".
#[tokio::test]
async fn an_idle_notification_is_not_an_approval() {
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

    let token = d.app.secrets.bot_token(&bot_id).expect("bot token");
    let http = reqwest::Client::new();
    let hook = format!("http://{}/hook?event=Notification", d.addr);
    let post = |body: Value| {
        http.post(&hook)
            .bearer_auth(token.clone())
            .json(&body)
            .send()
    };

    let idle = post(json!({
        "hook_event_name": "Notification",
        "message": "Claude is waiting for your input"
    }))
    .await
    .expect("hook post");
    assert!(idle.status().is_success(), "{:?}", idle.status());
    assert_eq!(d.app.supervisor.state(&bot_id).0.as_str(), "ready");

    let permission = post(json!({
        "hook_event_name": "Notification",
        "message": "Claude needs your permission to use Bash"
    }))
    .await
    .expect("hook post");
    assert!(
        permission.status().is_success(),
        "{:?}",
        permission.status()
    );
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "waiting_for_approval")
        .await;
}

/// Granting a permission fires no hook, so a bot that was approved and went
/// back to work used to keep showing "waiting for approval" until its turn
/// ended. A finished tool call is what says the prompt is gone.
#[tokio::test]
async fn a_tool_running_again_clears_a_pending_approval() {
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

    let token = d.app.secrets.bot_token(&bot_id).expect("bot token");
    let http = reqwest::Client::new();
    let hook = format!("http://{}/hook", d.addr);
    let post = |body: Value| {
        http.post(&hook)
            .bearer_auth(token.clone())
            .json(&body)
            .send()
    };

    let permission = post(json!({
        "event": "Notification",
        "message": "Claude needs your permission to use Bash"
    }))
    .await
    .expect("hook post");
    assert!(permission.status().is_success());
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "waiting_for_approval")
        .await;

    let ran = post(json!({"event": "PostToolUse"}))
        .await
        .expect("hook post");
    assert!(ran.status().is_success(), "{:?}", ran.status());
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "working")
        .await;

    // A tool running while the bot is already working is not a state change:
    // the turn-complete hook still decides when it goes back to ready.
    post(json!({"event": "PostToolUse"}))
        .await
        .expect("hook post");
    assert_eq!(d.app.supervisor.state(&bot_id).0.as_str(), "working");
}

/// A bot is one continuous conversation. Whatever ended the session — a crash,
/// or the daemon itself going away — the next one continues where it left off
/// instead of waking up blank.
#[tokio::test]
async fn a_restarted_bot_resumes_its_previous_session() {
    let d = spawn_daemon_with(|cfg| cfg.supervision_interval_ms = 100).await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let bot = create_bot(&mut c, &project_id, "alice").await;
    let bot_id = bot["id"].as_str().expect("bid").to_string();
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;

    let first = terminal(&d, &bot_id);
    assert!(
        !first.contains("--continue"),
        "the first session has nothing to continue: {first}"
    );

    c.send(json!({"type": "input", "bot_id": bot_id, "data": "\u{4}"}))
        .await;
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "crashed")
        .await;
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;

    let after = terminal(&d, &bot_id);
    assert!(
        after.contains("--continue"),
        "the restarted session must resume the old one: {after}"
    );
    // Recorded in the database, so a restarted daemon resumes too.
    assert!(d.app.db.bot_has_session(&bot_id).expect("flag"));
}

/// `/clear` ends the session and opens the next one inside the same process.
/// Marking the bot stopped on `SessionEnd` left a running terminal the UI
/// refused to type into.
#[tokio::test]
async fn clearing_a_session_leaves_the_bot_running() {
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

    let token = d.app.secrets.bot_token(&bot_id).expect("bot token");
    let http = reqwest::Client::new();
    let hook = |event: &str| {
        http.post(format!("http://{}/hook?event={event}", d.addr))
            .bearer_auth(token.clone())
            .json(&json!({}))
            .send()
    };

    let ended = hook("SessionEnd").await.expect("hook post");
    assert!(ended.status().is_success(), "{:?}", ended.status());
    assert_eq!(d.app.supervisor.state(&bot_id).0.as_str(), "ready");

    // The session that replaces it reports itself; the bot stays writable.
    let started = hook("SessionStart").await.expect("hook post");
    assert!(started.status().is_success(), "{:?}", started.status());
    assert_eq!(d.app.supervisor.state(&bot_id).0.as_str(), "ready");
    c.send(json!({"type": "input", "bot_id": bot_id, "data": "hi"}))
        .await;
}
