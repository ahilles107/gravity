//! Permission prompts answered from the app: Claude Code's PermissionRequest
//! hook waits on `/hook/permission` until the owner answers the card, and the
//! answer comes back as the hook's decision. Nothing defaults to yes.

mod common;

use common::*;
use serde_json::{json, Value};

struct Bot {
    d: TestDaemon,
    c: WsClient,
    bot_id: String,
    token: String,
}

async fn bot_with(tweak: impl FnOnce(&mut gravityd::config::Config)) -> Bot {
    let d = spawn_daemon_with(tweak).await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let bot = create_bot(&mut c, &project_id, "dev").await;
    let bot_id = bot["id"].as_str().expect("id").to_string();
    let token = d.app.secrets.bot_token(&bot_id).expect("token");
    Bot {
        d,
        c,
        bot_id,
        token,
    }
}

fn prompt() -> Value {
    json!({
        "hook_event_name": "PermissionRequest",
        "session_id": "s", "transcript_path": "/t.jsonl", "cwd": "/w",
        "tool_name": "Bash",
        "tool_input": {"command": "rm -rf build", "description": "Clean the build"},
        "permission_suggestions": [{
            "type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": "rm -rf build"}],
            "behavior": "allow", "destination": "localSettings"
        }]
    })
}

/// Starts the hook call as Claude Code would, without waiting for it.
fn hook(b: &Bot) -> tokio::task::JoinHandle<(u16, String)> {
    let url = format!("http://{}/hook/permission", b.d.addr);
    let token = b.token.clone();
    tokio::spawn(async move {
        let reply = reqwest::Client::new()
            .post(url)
            .bearer_auth(token)
            .json(&prompt())
            .send()
            .await
            .expect("hook post");
        let status = reply.status().as_u16();
        (status, reply.text().await.expect("body"))
    })
}

async fn card(b: &mut Bot) -> Value {
    let push = b.c.wait_for(|v| v["type"] == "permission_request").await;
    push["request"].clone()
}

#[tokio::test]
async fn allow_once_from_the_card_decides_the_prompt() {
    let mut b = bot_with(|_| {}).await;
    let call = hook(&b);
    let request = card(&mut b).await;
    assert_eq!(request["tool"], "Bash");
    assert_eq!(request["summary"], "Bash: rm -rf build");
    assert_eq!(
        b.d.app.supervisor.state(&b.bot_id).0,
        bus::BotState::WaitingForApproval
    );
    let listed =
        b.c.request(json!({"type": "list_permissions", "bot_id": b.bot_id}))
            .await;
    assert_eq!(listed["permissions"][0]["id"], request["id"]);

    let answered = b
        .c
        .request(json!({"type": "answer_permission", "request_id": request["id"], "decision": "allow_once"}))
        .await;
    assert_eq!(answered["type"], "permission", "{answered}");

    let (status, body) = call.await.expect("hook task");
    assert_eq!(status, 200);
    let output: Value = serde_json::from_str(&body).expect("hook output json");
    assert_eq!(
        output["hookSpecificOutput"]["hookEventName"],
        "PermissionRequest"
    );
    assert_eq!(
        output["hookSpecificOutput"]["decision"]["behavior"],
        "allow"
    );
    assert!(output["hookSpecificOutput"]["decision"]
        .get("updatedPermissions")
        .is_none());

    let resolved = b.c.wait_for(|v| v["type"] == "permission_resolved").await;
    assert_eq!(resolved["outcome"], "allowed_once");
    assert!(b.d.app.approvals.list(None).is_empty());
}

#[tokio::test]
async fn allow_for_the_session_never_writes_a_settings_file() {
    let mut b = bot_with(|_| {}).await;
    let call = hook(&b);
    let request = card(&mut b).await;
    b.c.request(json!({"type": "answer_permission", "request_id": request["id"], "decision": "allow_session"}))
        .await;
    let (_, body) = call.await.expect("hook task");
    let output: Value = serde_json::from_str(&body).expect("json");
    let rules = &output["hookSpecificOutput"]["decision"]["updatedPermissions"];
    assert_eq!(rules[0]["destination"], "session");
    assert_eq!(rules[0]["rules"][0]["ruleContent"], "rm -rf build");
}

#[tokio::test]
async fn deny_carries_the_owners_reason_and_a_second_answer_is_refused() {
    let mut b = bot_with(|_| {}).await;
    let call = hook(&b);
    let request = card(&mut b).await;
    b.c.request(json!({
        "type": "answer_permission", "request_id": request["id"],
        "decision": "deny", "reason": "keep the build cache"
    }))
    .await;
    let (_, body) = call.await.expect("hook task");
    let output: Value = serde_json::from_str(&body).expect("json");
    let decision = &output["hookSpecificOutput"]["decision"];
    assert_eq!(decision["behavior"], "deny");
    assert_eq!(decision["message"], "keep the build cache");

    let again = b
        .c
        .request(json!({"type": "answer_permission", "request_id": request["id"], "decision": "allow_once"}))
        .await;
    assert_eq!(again["type"], "error");
}

#[tokio::test]
async fn an_unanswered_prompt_is_denied_when_its_window_closes() {
    let mut b = bot_with(|cfg| cfg.permission_timeout_seconds = 1).await;
    let call = hook(&b);
    card(&mut b).await;
    let (_, body) = call.await.expect("hook task");
    let output: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(output["hookSpecificOutput"]["decision"]["behavior"], "deny");
    let resolved = b.c.wait_for(|v| v["type"] == "permission_resolved").await;
    assert_eq!(resolved["outcome"], "expired");
}

#[tokio::test]
async fn a_hook_that_gives_up_takes_its_card_away() {
    let mut b = bot_with(|_| {}).await;
    let call = hook(&b);
    card(&mut b).await;
    call.abort();
    let resolved = b.c.wait_for(|v| v["type"] == "permission_resolved").await;
    assert_eq!(resolved["outcome"], "abandoned");
    assert!(b.d.app.approvals.list(None).is_empty());
}

#[tokio::test]
async fn only_a_bot_token_can_ask() {
    let b = bot_with(|_| {}).await;
    let status = reqwest::Client::new()
        .post(format!("http://{}/hook/permission", b.d.addr))
        .bearer_auth("not-a-token")
        .json(&prompt())
        .send()
        .await
        .expect("post")
        .status();
    assert_eq!(status.as_u16(), 401);
}

#[test]
fn the_hook_is_installed_with_a_timeout_longer_than_the_window() {
    let dir = tempfile::tempdir().expect("tmp");
    gravityd::paths::write_hook_settings(dir.path(), 49777, "GRAVITY_TOKEN").expect("write");
    let settings: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(".claude/settings.json")).expect("read"),
    )
    .expect("json");
    let hook = &settings["hooks"]["PermissionRequest"][0]["hooks"][0];
    assert_eq!(hook["timeout"], gravityd::approval::HOOK_TIMEOUT_SECS);
    assert!(hook["command"]
        .as_str()
        .expect("command")
        .contains("/hook/permission"));
}

#[tokio::test]
async fn with_no_app_to_answer_the_prompt_stays_in_the_terminal() {
    let b = bot_with(|_| {}).await;
    // The harness client says it shows cards; drop it, leaving no answerer.
    drop(b.c);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let reply = reqwest::Client::new()
        .post(format!("http://{}/hook/permission", b.d.addr))
        .bearer_auth(&b.token)
        .json(&prompt())
        .send()
        .await
        .expect("post");
    assert_eq!(reply.status().as_u16(), 200);
    assert_eq!(
        reply.text().await.expect("body"),
        "",
        "no decision: Claude Code asks in the terminal"
    );
    assert!(b.d.app.approvals.list(None).is_empty());
}

#[tokio::test]
async fn a_runtime_permission_is_answered_through_its_session() {
    let mut b = bot_with(|_| {}).await;
    b.c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;
    // Ctrl-P makes the runtime double ask, as a Codex approval would.
    b.c.send(json!({"type": "input", "bot_id": b.bot_id, "data": "\u{10}"}))
        .await;
    let request = card(&mut b).await;
    assert_eq!(request["summary"], "Bash: echo from the double");
    b.c.request(json!({"type": "answer_permission", "request_id": request["id"], "decision": "allow_session"}))
        .await;
    let d = &b.d;
    let bot_id = b.bot_id.clone();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !terminal(d, &bot_id).contains("[permission 1: Session]") {
        assert!(
            tokio::time::Instant::now() < deadline,
            "answer never reached the session"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}
