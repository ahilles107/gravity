//! The model a bot talks with survives a restart.
//!
//! Claude Code reads its model from the user-global settings unless the
//! session was spawned with `--model`, so a bot used to change model behind
//! the user's back the moment anything restarted it. The daemon reads the
//! model out of the session's transcript and pins it on the next start.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{create_bot, spawn_daemon_with, terminal, TestDaemon, WsClient};
use serde_json::{json, Value};

/// Claude Code's transcript path for a workspace: every `/` and `.` becomes `-`.
fn transcript_dir(user_home: &Path, workspace: &str) -> PathBuf {
    let mangled: String = workspace
        .chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect();
    user_home.join(".claude").join("projects").join(mangled)
}

fn transcript(user_home: &Path, workspace: &str) -> PathBuf {
    transcript_dir(user_home, workspace).join("session.jsonl")
}

fn append(user_home: &Path, workspace: &str, entry: Value) {
    let dir = transcript_dir(user_home, workspace);
    fs::create_dir_all(&dir).expect("mkdir");
    let path = transcript(user_home, workspace);
    let mut body = fs::read_to_string(&path).unwrap_or_default();
    body.push_str(&entry.to_string());
    body.push('\n');
    fs::write(&path, body).expect("write transcript");
}

/// The model attachment Claude Code writes when a session names the model it
/// is running — what a `/model` in the terminal leaves behind.
fn write_model(user_home: &Path, workspace: &str, model_id: &str) {
    append(
        user_home,
        workspace,
        json!({
            "type": "attachment",
            "attachment": { "type": "model", "identity": { "modelId": model_id } }
        }),
    );
}

/// A daemon whose view of the user's home is a temp tree we can write
/// transcripts into, restarting bots briskly.
async fn daemon_with_fake_user_home() -> (TestDaemon, PathBuf) {
    let user_home = tempfile::tempdir().expect("tempdir").keep();
    let home = user_home.clone();
    let d = spawn_daemon_with(move |cfg| {
        cfg.user_home = home;
        cfg.supervision_interval_ms = 100;
    })
    .await;
    (d, user_home)
}

async fn ready_bot(c: &mut WsClient, name: &str) -> (String, String) {
    let project = c
        .request(json!({"type": "create_project", "name": name}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid");
    let bot = create_bot(c, project_id, name).await;
    let bot_id = bot["id"].as_str().expect("bid").to_string();
    let workspace = bot["workspace_path"]
        .as_str()
        .expect("workspace path")
        .to_string();
    c.wait_for(|v: &Value| v["type"] == "bot_state" && v["state"] == "ready")
        .await;
    (bot_id, workspace)
}

/// The runtime double opens every session with the flags it was spawned with,
/// so the greetings are the record of what the daemon asked for.
fn greetings(d: &TestDaemon, bot_id: &str) -> Vec<String> {
    terminal(d, bot_id)
        .lines()
        .filter(|line| line.contains("double runtime ready"))
        .map(str::to_string)
        .collect()
}

fn newest_greeting(d: &TestDaemon, bot_id: &str) -> String {
    greetings(d, bot_id).pop().expect("a session greeting")
}

/// Ends the session and waits for the one that replaces it. Polled rather than
/// driven off state pushes: crash backoff grows with the streak, and one of
/// these tests wants a streak.
async fn restart(d: &TestDaemon, c: &mut WsClient, bot_id: &str) {
    let before = greetings(d, bot_id).len();
    c.send(json!({"type": "input", "bot_id": bot_id, "data": "\u{4}"}))
        .await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if greetings(d, bot_id).len() > before
            && d.app.supervisor.state(bot_id).0.as_str() == "ready"
        {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the bot never came back"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The regression this exists for: pick a model in the bot's terminal, and the
/// next session comes back on it rather than on whatever the global settings
/// file has become.
#[tokio::test]
async fn the_model_a_bot_last_ran_with_is_pinned_on_the_next_start() {
    let (d, user_home) = daemon_with_fake_user_home().await;
    let mut c = WsClient::connect(&d).await;
    let (bot_id, workspace) = ready_bot(&mut c, "alice").await;

    // A bot that has never run names no model: its first session takes
    // whatever Claude Code's own default is.
    assert!(
        !newest_greeting(&d, &bot_id).contains("--model"),
        "nothing is pinned before a session has said what it runs"
    );
    assert_eq!(d.app.db.bot_model(&bot_id).expect("model"), None);

    write_model(&user_home, &workspace, "claude-opus-5[1m]");
    restart(&d, &mut c, &bot_id).await;

    let greeting = newest_greeting(&d, &bot_id);
    assert!(
        greeting.contains("--model claude-opus-5[1m]"),
        "the restarted session must come back on the same model: {greeting}"
    );
    // Stored, so a restarted daemon pins it too.
    assert_eq!(
        d.app.db.bot_model(&bot_id).expect("model").as_deref(),
        Some("claude-opus-5[1m]")
    );
}

/// The pin follows the user: `/model` in the terminal is a new choice, and the
/// session after it inherits that one instead of the old pin.
#[tokio::test]
async fn a_model_change_in_the_terminal_becomes_the_new_pin() {
    let (d, user_home) = daemon_with_fake_user_home().await;
    let mut c = WsClient::connect(&d).await;
    let (bot_id, workspace) = ready_bot(&mut c, "alice").await;

    write_model(&user_home, &workspace, "claude-opus-5[1m]");
    restart(&d, &mut c, &bot_id).await;
    write_model(&user_home, &workspace, "claude-fable-5-1");
    restart(&d, &mut c, &bot_id).await;

    let greeting = newest_greeting(&d, &bot_id);
    assert!(
        greeting.contains("--model claude-fable-5-1"),
        "the newest choice wins: {greeting}"
    );
    assert_eq!(
        d.app.db.bot_model(&bot_id).expect("model").as_deref(),
        Some("claude-fable-5-1")
    );
}

/// A turn names the model without its context-window suffix, and re-pinning on
/// that would quietly move a 1M-context bot onto the 200k variant.
#[tokio::test]
async fn a_turn_naming_the_base_model_does_not_narrow_the_pin() {
    let (d, user_home) = daemon_with_fake_user_home().await;
    let mut c = WsClient::connect(&d).await;
    let (bot_id, workspace) = ready_bot(&mut c, "alice").await;

    write_model(&user_home, &workspace, "claude-opus-5[1m]");
    restart(&d, &mut c, &bot_id).await;
    append(
        &user_home,
        &workspace,
        json!({ "type": "assistant", "message": { "model": "claude-opus-5" } }),
    );
    restart(&d, &mut c, &bot_id).await;

    let greeting = newest_greeting(&d, &bot_id);
    assert!(
        greeting.contains("--model claude-opus-5[1m]"),
        "the suffixed pin must hold: {greeting}"
    );
}

/// A model the runtime will not accept would crash-loop a bot nothing can
/// start by hand, so a crash streak under a pin drops the pin.
#[tokio::test]
async fn a_crash_streak_drops_the_pin() {
    let (d, user_home) = daemon_with_fake_user_home().await;
    let mut c = WsClient::connect(&d).await;
    let (bot_id, workspace) = ready_bot(&mut c, "alice").await;

    write_model(&user_home, &workspace, "claude-retired-1");
    for _ in 0..3 {
        restart(&d, &mut c, &bot_id).await;
    }

    assert_eq!(
        d.app.db.bot_model(&bot_id).expect("model"),
        None,
        "the pin is gone after the streak"
    );
    let greeting = newest_greeting(&d, &bot_id);
    assert!(
        !greeting.contains("--model"),
        "the recovering session starts unpinned: {greeting}"
    );
}
