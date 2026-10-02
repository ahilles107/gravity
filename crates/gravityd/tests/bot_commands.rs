//! A bot's commands: what it is running and has run, read from its
//! transcript, with a running background command's live output, here and from
//! the other machine.

mod common;

use common::peers::team;
use common::*;
use serde_json::{json, Value};

/// Writes a Claude Code transcript for a bot on `d`.
fn transcript(d: &TestDaemon, bot_id: &str, records: &[Value]) {
    let bot = d.app.db.get_bot(bot_id).expect("db").expect("bot");
    let mangled: String = bot
        .workspace_path
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let dir = d.app.cfg.user_home.join(".claude/projects").join(mangled);
    std::fs::create_dir_all(&dir).expect("dir");
    let lines: Vec<String> = records.iter().map(Value::to_string).collect();
    std::fs::write(dir.join("session.jsonl"), lines.join("\n") + "\n").expect("write");
}

/// A finished test run, and a build still running in the background whose
/// output goes to `output`.
fn records(output: &std::path::Path) -> Vec<Value> {
    let at = "2026-10-01T10:00:00Z";
    vec![
        json!({"type": "user", "uuid": "turn-1", "sessionId": "s1", "timestamp": at,
               "isMeta": true, "origin": {"kind": "peer"},
               "message": {"content": "Another Claude session sent a message:\n[msg #7 from USER · chat] build it"}}),
        json!({"type": "assistant", "uuid": "a1", "sessionId": "s1", "timestamp": at,
               "message": {"content": [
                   {"type": "tool_use", "id": "t1", "name": "Bash",
                    "input": {"command": "cargo test", "description": "Run tests"}},
                   {"type": "tool_use", "id": "b1", "name": "Bash",
                    "input": {"command": "./build.sh", "description": "Build the player",
                              "run_in_background": true}}]}}),
        json!({"type": "user", "uuid": "r1", "sessionId": "s1", "timestamp": "2026-10-01T10:00:09Z",
               "message": {"content": [
                   {"type": "tool_result", "tool_use_id": "t1", "content": "test result: ok. 12 passed"},
                   {"type": "tool_result", "tool_use_id": "b1", "content": format!(
                       "Command running in background with ID: bx1. Output is being written to: {}. You will be notified when it completes.",
                       output.display())}]}}),
    ]
}

fn background_output(dir: &std::path::Path) -> std::path::PathBuf {
    let tasks = dir.join("tasks");
    std::fs::create_dir_all(&tasks).expect("tasks");
    let output = tasks.join("bx1.output");
    std::fs::write(&output, "compiling…\nlinking player\n").expect("output");
    output
}

#[tokio::test]
async fn a_bots_commands_are_listed_running_first_with_live_output() {
    let d = spawn_daemon_with(|cfg| cfg.user_home = cfg.home.join("user")).await;
    let mut c = WsClient::connect(&d).await;
    let created = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let pid = created["project"]["id"].as_str().expect("pid").to_string();
    let bot = create_bot(&mut c, &pid, "builder").await;
    let id = bot["id"].as_str().expect("id").to_string();
    let output = background_output(&d.app.cfg.home);
    transcript(&d, &id, &records(&output));

    let listed = c
        .request(json!({"type": "list_bot_commands", "bot_id": id}))
        .await;
    assert_eq!(listed["type"], "bot_commands", "{listed}");
    let commands = listed["commands"].as_array().expect("commands");
    assert_eq!(commands.len(), 2, "{listed}");
    let build = &commands[0];
    assert_eq!(build["status"], "running");
    assert_eq!(build["background"], true);
    assert_eq!(build["description"], "Build the player");
    assert_eq!(build["task_id"], "bx1");
    assert!(build["output"]
        .as_str()
        .expect("output")
        .contains("linking player"));
    let tests = &commands[1];
    assert_eq!(tests["status"], "done");
    assert_eq!(tests["command"], "cargo test");
    assert_eq!(tests["output"], "test result: ok. 12 passed");
}

#[tokio::test]
async fn the_other_machines_bot_commands_are_read_from_there() {
    let mut t = team().await;
    let output = background_output(&t.win.app.cfg.home);
    transcript(&t.win, &t.windev_id, &records(&output));
    let listed = t
        .mac_client
        .request(json!({"type": "list_bot_commands", "bot_id": t.linked_windev}))
        .await;
    assert_eq!(listed["type"], "bot_commands", "{listed}");
    assert_eq!(listed["bot_id"], t.linked_windev.as_str());
    assert_eq!(listed["commands"][0]["status"], "running");
    assert!(listed["commands"][0]["output"]
        .as_str()
        .expect("output")
        .contains("linking player"));
}
