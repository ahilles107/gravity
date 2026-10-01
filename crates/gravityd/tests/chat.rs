//! The chat pane's daemon side: a bot's transcript served as turns over the
//! control plane, followed live as it grows, with step detail, images and
//! files on demand.

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};

use common::*;
use serde_json::{json, Value};

struct Setup {
    d: TestDaemon,
    c: WsClient,
    bot_id: String,
    project_id: String,
    transcript: PathBuf,
}

/// A daemon whose Claude Code transcripts live under its own temp home, and
/// one bot with a transcript file to write into.
async fn setup() -> Setup {
    let d = spawn_daemon_with(|cfg| cfg.user_home = cfg.home.join("user")).await;
    let mut c = WsClient::connect(&d).await;
    let created = c
        .request(json!({"type": "create_project", "name": "app"}))
        .await;
    let project_id = created["project"]["id"].as_str().expect("pid").to_string();
    let bot = create_bot(&mut c, &project_id, "dev").await;
    let bot_id = bot["id"].as_str().expect("id").to_string();
    let workspace = bot["workspace_path"]
        .as_str()
        .expect("workspace")
        .to_string();
    let mangled: String = workspace
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let dir = d.app.cfg.user_home.join(".claude/projects").join(mangled);
    std::fs::create_dir_all(&dir).expect("transcript dir");
    Setup {
        transcript: dir.join("session.jsonl"),
        d,
        c,
        bot_id,
        project_id,
    }
}

fn append(path: &Path, records: &[Value]) {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open transcript");
    for record in records {
        writeln!(file, "{record}").expect("write");
    }
}

fn owner_chat(text: &str) -> Value {
    json!({"type": "user", "uuid": "turn-1", "timestamp": "2026-10-01T10:00:00Z",
           "isMeta": true, "origin": {"kind": "peer"},
           "message": {"content": format!(
               "Another Claude session sent a message:\n[msg #7 from USER · chat] {text}")}})
}

#[tokio::test]
async fn a_transcript_is_served_as_turns_and_followed_live() {
    let mut s = setup().await;
    append(
        &s.transcript,
        &[
            owner_chat("screenshot the login page"),
            json!({"type": "assistant", "uuid": "a1", "timestamp": "2026-10-01T10:00:02Z",
                   "message": {"content": [
                       {"type": "text", "text": "Taking it now."},
                       {"type": "tool_use", "id": "shot", "name": "mcp__chrome__screenshot",
                        "input": {"url": "http://localhost"}}]}}),
            json!({"type": "user", "uuid": "r1", "timestamp": "2026-10-01T10:00:03Z",
                   "message": {"content": [{"type": "tool_result", "tool_use_id": "shot",
                       "content": [{"type": "image", "source": {"type": "base64",
                           "media_type": "image/png", "data": "iVBORw0KGgo="}}]}]}}),
        ],
    );

    let chat =
        s.c.request(json!({"type": "list_chat", "bot_id": s.bot_id}))
            .await;
    assert_eq!(chat["type"], "chat", "{chat}");
    let turn = &chat["turns"][0];
    assert_eq!(turn["trigger"]["kind"], "owner");
    assert_eq!(turn["trigger"]["via"], "chat");
    assert_eq!(turn["items"][0]["markdown"], "Taking it now.");
    let image = &turn["items"][1]["images"][0];
    assert_eq!(image["mime"], "image/png");

    let file =
        s.c.request(json!({"type": "get_chat_image", "bot_id": s.bot_id, "image_id": image["id"]}))
            .await;
    assert_eq!(file["file"]["base64"], "iVBORw0KGgo=");

    let step =
        s.c.request(json!({"type": "get_chat_step", "bot_id": s.bot_id, "item_id": "shot"}))
            .await;
    assert!(step["detail"]["input"]
        .as_str()
        .expect("input")
        .contains("localhost"));

    // The bot keeps going: the new text arrives as a push for the same turn.
    append(
        &s.transcript,
        &[
            json!({"type": "assistant", "uuid": "a2", "timestamp": "2026-10-01T10:00:04Z",
                 "message": {"content": [{"type": "text", "text": "Here it is."}]}}),
        ],
    );
    let pushed =
        s.c.wait_for(|v| v["type"] == "chat_turns" && v["bot_id"] == json!(s.bot_id.clone()))
            .await;
    assert_eq!(pushed["turns"][0]["id"], "turn-1");
    let items = pushed["turns"][0]["items"].as_array().expect("items");
    assert_eq!(items.last().expect("item")["markdown"], "Here it is.");
}

#[tokio::test]
async fn files_are_confined_to_the_bot_and_the_project_artifacts() {
    let mut s = setup().await;
    let project =
        s.d.app
            .db
            .get_project(&s.project_id)
            .expect("db")
            .expect("project");
    let artifacts = gravityd::paths::artifacts_dir(&s.d.app.cfg, &project.dir_name);
    std::fs::write(artifacts.join("report.md"), "# Weekly report\n\nAll green.").expect("write");

    let listed =
        s.c.request(json!({"type": "list_artifacts", "project_id": s.project_id}))
            .await;
    let entry = &listed["artifacts"][0];
    assert_eq!(entry["name"], "report.md");
    assert_eq!(entry["title"], "Weekly report");
    assert_eq!(entry["mime"], "text/markdown");

    let read =
        s.c.request(json!({"type": "read_file", "project_id": s.project_id, "path": "report.md"}))
            .await;
    assert_eq!(read["file"]["text"], "# Weekly report\n\nAll green.");

    // The bot scope reaches its own workspace and the shared artifacts...
    let read =
        s.c.request(json!({"type": "read_file", "bot_id": s.bot_id, "path": "CLAUDE.md"}))
            .await;
    assert_eq!(read["type"], "file", "{read}");
    let read =
        s.c.request(json!({
            "type": "read_file", "bot_id": s.bot_id,
            "path": artifacts.join("report.md").display().to_string()
        }))
        .await;
    assert_eq!(read["type"], "file", "{read}");

    // ...and nothing else, however the path is written.
    for escape in ["../../../../secrets/client.token", "/etc/hosts"] {
        let refused =
            s.c.request(json!({"type": "read_file", "bot_id": s.bot_id, "path": escape}))
                .await;
        assert_eq!(
            refused["type"], "error",
            "{escape} must be refused: {refused}"
        );
    }
}

#[tokio::test]
async fn the_chat_capability_is_advertised() {
    let d = spawn_daemon().await;
    let hello = raw_hello(&d, d.app.secrets.client_token()).await;
    let caps = hello["capabilities"].as_array().expect("capabilities");
    assert!(caps.contains(&json!("chat")));
}

#[tokio::test]
async fn attachments_upload_in_chunks_into_the_project_artifacts() {
    use base64::Engine;
    let mut s = setup().await;
    let encode = |text: &str| base64::engine::general_purpose::STANDARD.encode(text);
    let first =
        s.c.request(json!({
            "type": "write_artifact", "project_id": s.project_id,
            "name": "../../evil/notes.txt", "base64": encode("hello "), "last": false
        }))
        .await;
    let upload_id = first["upload"]["upload_id"]
        .as_str()
        .expect("upload id")
        .to_string();
    assert!(first["upload"]["path"].is_null());
    let done =
        s.c.request(json!({
            "type": "write_artifact", "project_id": s.project_id, "upload_id": upload_id,
            "name": "../../evil/notes.txt", "base64": encode("world"), "last": true
        }))
        .await;
    let path = done["upload"]["path"].as_str().expect("path");
    assert!(path.ends_with("artifacts/uploads/notes.txt"), "{path}");
    assert_eq!(std::fs::read_to_string(path).expect("read"), "hello world");

    let refused =
        s.c.request(json!({
            "type": "write_artifact", "project_id": s.project_id, "upload_id": "../x",
            "name": "a.txt", "base64": encode("x")
        }))
        .await;
    assert_eq!(refused["type"], "error");
}
