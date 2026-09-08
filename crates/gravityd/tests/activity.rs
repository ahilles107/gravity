//! Sidebar preview lines.
//!
//! A bot's conversation with the user happens in its Claude Code terminal, a
//! raw pty that never produces a bus message, so the daemon reads it out of
//! Claude Code's JSONL transcript. The delicate part is *when*: the `Stop` hook
//! that ends a turn fires before Claude Code has written the reply, so anything
//! that reads on that signal alone gets the previous turn forever.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{spawn_daemon_with, TestDaemon, WsClient};
use serde_json::{json, Value};

/// Claude Code's transcript path for a workspace: every `/` and `.` becomes `-`.
fn transcript_dir(user_home: &Path, workspace: &str) -> PathBuf {
    let mangled: String = workspace
        .chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect();
    user_home.join(".claude").join("projects").join(mangled)
}

/// Appends one assistant turn to a bot's transcript, as Claude Code would.
fn write_turn(user_home: &Path, workspace: &str, text: &str, at: &str) {
    let dir = transcript_dir(user_home, workspace);
    fs::create_dir_all(&dir).expect("mkdir");
    let line = json!({
        "type": "assistant",
        "timestamp": at,
        "message": { "content": [{ "type": "text", "text": text }] }
    })
    .to_string();
    let path = dir.join("session.jsonl");
    let mut body = fs::read_to_string(&path).unwrap_or_default();
    body.push_str(&line);
    body.push('\n');
    fs::write(&path, body).expect("write transcript");
}

async fn project(c: &mut WsClient, name: &str) -> String {
    let reply = c
        .request(json!({ "type": "create_project", "name": name }))
        .await;
    reply["project"]["id"]
        .as_str()
        .expect("project id")
        .to_string()
}

/// A daemon whose view of the user's home is a temp tree we can write
/// transcripts into.
async fn daemon_with_fake_user_home() -> (TestDaemon, PathBuf) {
    let user_home = tempfile::tempdir().expect("tempdir").keep();
    let home = user_home.clone();
    let d = spawn_daemon_with(move |cfg| cfg.user_home = home).await;
    (d, user_home)
}

fn workspace_of(bot: &Value) -> String {
    bot["workspace_path"]
        .as_str()
        .expect("workspace path")
        .to_string()
}

/// A bot row without the side effects of creating one through the daemon,
/// which queues an introduction prompt — a bus message, and therefore
/// activity. These two tests are about what the transcript says, so the bot
/// has to start out with nothing on the bus at all.
fn quiet_bot(d: &TestDaemon, project_id: &str, name: &str) -> (String, String) {
    let dir_name = name.to_lowercase();
    let workspace = d
        .app
        .cfg
        .home
        .join(&dir_name)
        .join("workspace")
        .display()
        .to_string();
    let bot = d
        .app
        .db
        .create_bot(project_id, name, "", "", "", &workspace, &dir_name, None)
        .expect("bot row");
    (bot.id, bot.workspace_path)
}

#[tokio::test]
async fn lists_the_newest_turn_per_bot() {
    let (d, user_home) = daemon_with_fake_user_home().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (bot_id, workspace) = quiet_bot(&d, &pid, "Scribe");

    write_turn(&user_home, &workspace, "older", "2026-01-01T00:00:00Z");
    write_turn(&user_home, &workspace, "newest", "2026-01-02T00:00:00Z");

    let reply = c.request(json!({ "type": "list_bot_activity" })).await;
    assert_eq!(reply["type"], "bot_activity", "{reply}");
    let items = reply["activity"].as_array().expect("activity array");
    let mine = items
        .iter()
        .find(|item| item["bot_id"] == json!(bot_id))
        .expect("an entry for the bot");
    assert_eq!(mine["text"], "newest");
    assert_eq!(mine["from"], "");
}

#[tokio::test]
async fn omits_a_bot_that_has_said_nothing() {
    let (d, _user_home) = daemon_with_fake_user_home().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (bot_id, _workspace) = quiet_bot(&d, &pid, "Silent");

    let reply = c.request(json!({ "type": "list_bot_activity" })).await;
    let items = reply["activity"].as_array().expect("activity array");
    assert!(
        !items.iter().any(|item| item["bot_id"] == json!(bot_id)),
        "a bot with no transcript and no messages must be omitted: {reply}"
    );
}

/// The regression this whole mechanism exists for. The turn's text is written
/// *after* the hook that ends the turn, exactly as Claude Code does it; the
/// daemon must wait for it rather than push whatever was already on disk.
#[tokio::test]
async fn pushes_the_finished_turn_even_though_the_transcript_lags_the_hook() {
    let (d, user_home) = daemon_with_fake_user_home().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let bot = common::create_bot(&mut c, &pid, "Scribe").await;
    let bot_id = bot["id"].as_str().expect("bot id").to_string();
    let workspace = workspace_of(&bot);

    // What the previous turn left behind: this is what a reader that does not
    // wait would report.
    write_turn(
        &user_home,
        &workspace,
        "previous turn",
        "2026-01-01T00:00:00Z",
    );

    let late_home = user_home.clone();
    let late_workspace = workspace.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let at = chrono::Utc::now().to_rfc3339();
        write_turn(&late_home, &late_workspace, "the fresh reply", &at);
    });

    d.app.supervisor.on_hook(&bot_id, "UserPromptSubmit", None);
    d.app.supervisor.on_hook(&bot_id, "Stop", None);

    let push = c.wait_for(|v| v["type"] == json!("activity_update")).await;
    assert_eq!(push["activity"]["bot_id"], json!(bot_id), "{push}");
    assert_eq!(
        push["activity"]["text"], "the fresh reply",
        "the push must carry the turn that just finished, not the one before it"
    );
}
