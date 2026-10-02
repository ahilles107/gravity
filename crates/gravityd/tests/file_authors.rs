//! The Files tab says who made each file in a project's artifacts.

mod common;

use common::peers::{find, team};
use common::tasks::drain_until;
use common::*;
use serde_json::{json, Value};

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

fn calls(at: &str, calls: Value) -> Value {
    json!({"type": "assistant", "timestamp": at, "message": {"content": calls}})
}

fn creator<'a>(listing: &'a Value, rel: &str) -> &'a Value {
    let artifact = listing["artifacts"]
        .as_array()
        .expect("artifacts")
        .iter()
        .find(|a| a["rel"] == rel)
        .unwrap_or_else(|| panic!("no {rel} in {listing}"));
    &artifact["created_by"]
}

#[tokio::test]
async fn each_file_names_the_bot_that_made_it() {
    let d = spawn_daemon_with(|cfg| cfg.user_home = cfg.home.join("user")).await;
    let mut c = WsClient::connect(&d).await;
    let created = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let pid = created["project"]["id"].as_str().expect("pid").to_string();
    let writer = create_bot(&mut c, &pid, "writer").await;
    let artist = create_bot(&mut c, &pid, "artist").await;
    let project = d.app.db.get_project(&pid).expect("db").expect("project");
    let root = gravityd::paths::artifacts_dir(&d.app.cfg, &project.dir_name);
    std::fs::create_dir_all(root.join("uploads")).expect("dirs");
    for name in ["report.md", "shot.png", "uploads/brief.pdf", "stray.txt"] {
        std::fs::write(root.join(name), "x").expect("write");
    }
    let report = root.join("report.md").display().to_string();
    let shot = root.join("shot.png").display().to_string();
    transcript(
        &d,
        writer["id"].as_str().expect("id"),
        &[calls(
            "2026-10-01T10:00:00Z",
            json!([{"type": "tool_use", "id": "w", "name": "Write",
                    "input": {"file_path": report, "content": "# Report"}}]),
        )],
    );
    transcript(
        &d,
        artist["id"].as_str().expect("id"),
        &[
            calls(
                "2026-10-01T09:00:00Z",
                json!([{"type": "tool_use", "id": "b", "name": "Bash",
                        "input": {"command": format!("cp render/out.png {shot}")}}]),
            ),
            // A later edit does not take the report from its writer.
            calls(
                "2026-10-01T11:00:00Z",
                json!([{"type": "tool_use", "id": "e", "name": "Edit",
                        "input": {"file_path": report, "old_string": "a", "new_string": "b"}}]),
            ),
        ],
    );

    let listing = c
        .request(json!({"type": "list_artifacts", "project_id": pid}))
        .await;
    assert_eq!(listing["type"], "artifacts", "{listing}");
    let report_by = creator(&listing, "report.md");
    assert_eq!(report_by["name"], "writer");
    assert_eq!(report_by["via"], "wrote");
    assert_eq!(report_by["bot_id"], writer["id"]);
    let shot_by = creator(&listing, "shot.png");
    assert_eq!(shot_by["name"], "artist");
    assert_eq!(shot_by["via"], "command");
    let upload_by = creator(&listing, "uploads/brief.pdf");
    assert_eq!(
        (upload_by["name"].as_str(), upload_by["via"].as_str()),
        (Some("you"), Some("upload"))
    );
    assert!(creator(&listing, "stray.txt").is_null());
}

#[tokio::test]
async fn a_file_sent_from_the_other_machine_names_its_bot() {
    let mut t = team().await;
    let sent = t
        .lead
        .call(
            "send_message",
            json!({"to": "windev", "kind": "task", "body": "build the installer"}),
        )
        .await;
    assert!(sent["task_id"].is_string());
    let inbox = drain_until(&mut t.windev, "build the installer").await;
    let task = find(&inbox, "build the installer")["task_id"]
        .as_str()
        .expect("task")
        .to_string();
    let windev = t
        .win
        .app
        .db
        .get_bot(&t.windev_id)
        .expect("db")
        .expect("bot");
    let out = std::path::Path::new(&windev.workspace_path).join("out");
    std::fs::create_dir_all(&out).expect("mkdir");
    std::fs::write(out.join("setup.txt"), "installer").expect("write");
    t.windev
        .call(
            "complete_task",
            json!({"task_id": task, "result": "built", "artifacts": ["out/setup.txt"]}),
        )
        .await;
    drain_until(&mut t.lead, "built").await;

    let pid = t
        .mac
        .app
        .db
        .get_bot(&t.linked_windev)
        .expect("db")
        .expect("bot")
        .project_id;
    let listing = t
        .mac_client
        .request(json!({"type": "list_artifacts", "project_id": pid}))
        .await;
    let sent_file = listing["artifacts"]
        .as_array()
        .expect("artifacts")
        .iter()
        .find(|a| a["name"] == "setup.txt")
        .unwrap_or_else(|| panic!("no transferred file in {listing}"));
    let by = &sent_file["created_by"];
    assert_eq!(by["name"], "windev", "{sent_file}");
    assert_eq!(by["machine"], "win");
    assert_eq!(by["via"], "sent");
}
