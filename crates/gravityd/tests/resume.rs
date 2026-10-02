//! After a restart, a bot cut off mid-turn, or still holding open tasks, is
//! told to pick its work back up, once.

mod common;

use common::*;
use serde_json::{json, Value};

/// Writes a Claude Code transcript for a bot, as its session would have.
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

fn bus_turn(id: &str, text: &str) -> Value {
    json!({"type": "user", "uuid": id, "timestamp": "2026-10-01T10:00:00Z",
           "isMeta": true, "origin": {"kind": "peer"},
           "message": {"content": format!(
               "Another Claude session sent a message:\n[msg #7 from LEAD · task] {text}")}})
}

/// Reads a bot's inbox until `needle` arrives, keeping everything, the
/// daemon's own notes included.
async fn read_until(client: &mut McpClient, needle: &str) -> Vec<Value> {
    let mut seen: Vec<Value> = Vec::new();
    for _ in 0..60 {
        let inbox = client.call("check_inbox", json!({})).await;
        seen.extend(inbox["messages"].as_array().cloned().unwrap_or_default());
        if seen
            .iter()
            .any(|m| m["body"].as_str().is_some_and(|b| b.contains(needle)))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    seen
}

struct Team {
    d: TestDaemon,
    ids: Vec<String>,
    bots: Vec<McpClient>,
}

/// A project with these bots, whose transcripts live under the test's home.
async fn team(names: &[&str]) -> Team {
    let d = spawn_daemon_with(|cfg| cfg.user_home = cfg.home.join("user")).await;
    let mut c = WsClient::connect(&d).await;
    let created = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let pid = created["project"]["id"].as_str().expect("pid").to_string();
    let (mut ids, mut bots) = (Vec::new(), Vec::new());
    for name in names {
        let bot = create_bot(&mut c, &pid, name).await;
        let id = bot["id"].as_str().expect("id").to_string();
        bots.push(McpClient::new(
            &d,
            &d.app.secrets.bot_token(&id).expect("token"),
        ));
        ids.push(id);
    }
    Team { d, ids, bots }
}

#[tokio::test]
async fn bots_cut_off_mid_task_are_told_to_pick_it_back_up() {
    let Team { d, ids, mut bots } = team(&["lead", "dev", "idle"]).await;
    let (dev, idle) = (ids[1].clone(), ids[2].clone());
    let sent = bots[0]
        .call(
            "send_message",
            json!({"to": "dev", "kind": "task", "body": "port the updater"}),
        )
        .await;
    let task_id = sent["task_id"].as_str().expect("task").to_string();
    read_until(&mut bots[1], "port the updater").await;

    // dev was working on it when the daemon stopped: the turn never ended.
    transcript(&d, &dev, &[bus_turn("t-dev", "port the updater")]);
    // idle finished its last turn and holds nothing.
    transcript(
        &d,
        &idle,
        &[
            bus_turn("t-idle", "say hi"),
            json!({"type": "system", "subtype": "turn_duration", "uuid": "end",
                   "timestamp": "2026-10-01T10:00:05Z", "durationMs": 5000}),
        ],
    );

    let work = gravityd::resume::interrupted_work(&d.app);
    let ids: Vec<&str> = work.iter().map(|w| w.bot_id.as_str()).collect();
    assert_eq!(ids, vec![dev.as_str()], "{work:?}");
    assert!(work[0].turn.is_some());
    assert_eq!(work[0].tasks[0].0, task_id);
    assert_eq!(work[0].tasks[0].1, "lead");

    // Two restarts in quick succession still tell it once.
    gravityd::resume::nudge(&d.app, work.clone());
    gravityd::resume::nudge(&d.app, work);
    let inbox = read_until(&mut bots[1], "Gravity restarted").await;
    // Long enough for a duplicate to have arrived too.
    let mut inbox = inbox;
    inbox.extend(read_until(&mut bots[1], "never sent").await);
    let notes: Vec<&Value> = inbox
        .iter()
        .filter(|m| {
            m["body"]
                .as_str()
                .is_some_and(|b| b.starts_with(gravityd::resume::HEADER))
        })
        .collect();
    assert_eq!(notes.len(), 1, "{inbox:?}");
    let body = notes[0]["body"].as_str().expect("body");
    assert!(body.contains(&format!("task_id {task_id}, from lead: port the updater")));
    assert!(
        body.contains("middle of a turn, started by lead's task: port the updater"),
        "{body}"
    );
}

#[tokio::test]
async fn resuming_can_be_turned_off() {
    let d = spawn_daemon_with(|cfg| {
        cfg.user_home = cfg.home.join("user");
        cfg.resume_after_restart = false;
    })
    .await;
    let mut c = WsClient::connect(&d).await;
    let created = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let pid = created["project"]["id"].as_str().expect("pid").to_string();
    let bot = create_bot(&mut c, &pid, "dev").await;
    transcript(
        &d,
        bot["id"].as_str().expect("id"),
        &[bus_turn("t", "work")],
    );
    assert!(gravityd::resume::interrupted_work(&d.app).is_empty());
}
