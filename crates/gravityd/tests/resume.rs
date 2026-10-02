//! A bot whose session restarts (the owner's Restart, a crash, a cleared
//! conversation, the daemon starting) is told what its last session left
//! unfinished, once, and keeps its work.

mod common;

use common::peers::{team, wait_until};
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

/// A turn started by lead's task that never finished.
fn cut_off(text: &str) -> Value {
    json!({"type": "user", "uuid": "t-dev", "timestamp": "2026-10-01T10:00:00Z",
           "isMeta": true, "origin": {"kind": "peer"},
           "message": {"content": format!(
               "Another Claude session sent a message:\n[msg #7 from LEAD · task] {text}")}})
}

/// Reads a bot's inbox until `needle` arrives, keeping everything, the
/// daemon's own notes included.
async fn read_until(client: &mut McpClient, needle: &str, tries: usize) -> Vec<Value> {
    let mut seen: Vec<Value> = Vec::new();
    for _ in 0..tries {
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

fn notes(inbox: &[Value]) -> Vec<&str> {
    inbox
        .iter()
        .filter_map(|m| m["body"].as_str())
        .filter(|b| b.starts_with(gravityd::resume::HEADER))
        .collect()
}

struct Team {
    d: TestDaemon,
    c: WsClient,
    dev: String,
    dev_mcp: McpClient,
    task_id: String,
}

/// lead and dev; lead has given dev a task, which dev was in the middle of.
async fn interrupted(tweak: impl FnOnce(&mut gravityd::config::Config)) -> Team {
    let d = spawn_daemon_with(|cfg| {
        cfg.user_home = cfg.home.join("user");
        cfg.supervision_interval_ms = 250;
        tweak(cfg);
    })
    .await;
    let mut c = WsClient::connect(&d).await;
    let created = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let pid = created["project"]["id"].as_str().expect("pid").to_string();
    let mut ids = Vec::new();
    for name in ["lead", "dev"] {
        let bot = create_bot(&mut c, &pid, name).await;
        ids.push(bot["id"].as_str().expect("id").to_string());
    }
    let mcp = |id: &str| McpClient::new(&d, &d.app.secrets.bot_token(id).expect("token"));
    let (mut lead, mut dev_mcp) = (mcp(&ids[0]), mcp(&ids[1]));
    let sent = lead
        .call(
            "send_message",
            json!({"to": "dev", "kind": "task", "body": "port the updater"}),
        )
        .await;
    read_until(&mut dev_mcp, "port the updater", 60).await;
    transcript(&d, &ids[1], &[cut_off("port the updater")]);
    Team {
        task_id: sent["task_id"].as_str().expect("task").to_string(),
        dev: ids[1].clone(),
        d,
        c,
        dev_mcp,
    }
}

#[tokio::test]
async fn restarting_a_bot_tells_it_to_pick_its_work_back_up_once() {
    let mut t = interrupted(|_| {}).await;
    // Two restarts in quick succession still tell it once.
    for _ in 0..2 {
        let ok =
            t.c.request(json!({"type": "restart_bot", "bot_id": t.dev}))
                .await;
        assert_eq!(ok["type"], "ok", "{ok}");
    }
    let mut inbox = read_until(&mut t.dev_mcp, "Your session restarted", 100).await;
    inbox.extend(read_until(&mut t.dev_mcp, "never sent", 20).await);
    let notes = notes(&inbox);
    assert_eq!(notes.len(), 1, "{inbox:?}");
    let note = notes[0];
    assert!(note.contains("picked your conversation back up"), "{note}");
    assert!(note.contains("middle of a turn, started by lead's task: port the updater"));
    assert!(note.contains(&format!("task_id {}, from lead", t.task_id)));
    // It came back resuming its conversation.
    let (d, dev) = (&t.d, t.dev.clone());
    wait_until("the session resumed", || {
        terminal(d, &dev).contains("--continue")
    })
    .await;
    // The work is still there to finish.
    let done = t
        .dev_mcp
        .call_raw(
            "complete_task",
            json!({"task_id": t.task_id, "result": "ported"}),
        )
        .await;
    assert_ne!(done["isError"], json!(true), "{done}");
}

#[tokio::test]
async fn clearing_a_bot_starts_fresh_and_says_so() {
    let mut t = interrupted(|_| {}).await;
    let ok =
        t.c.request(json!({"type": "clear_bot_session", "bot_id": t.dev}))
            .await;
    assert_eq!(ok["type"], "ok", "{ok}");
    let inbox = read_until(&mut t.dev_mcp, "Your session restarted", 100).await;
    let note = notes(&inbox).first().copied().expect("a note").to_string();
    assert!(note.contains("cleared conversation"), "{note}");
    assert!(note.contains("FACTS.md"));
    assert!(note.contains(&format!("task_id {}", t.task_id)));
    // The new session does not pick the old conversation back up.
    let (d, dev) = (&t.d, t.dev.clone());
    wait_until("a fresh session started", || {
        terminal(d, &dev)
            .matches("double runtime ready for dev")
            .count()
            >= 2
    })
    .await;
    let screen = terminal(&t.d, &t.dev);
    let last = screen
        .rsplit("double runtime ready for dev")
        .next()
        .unwrap_or_default();
    let flags = last.lines().next().unwrap_or_default();
    assert!(!flags.contains("--continue"), "{flags}");
}

#[tokio::test]
async fn a_crash_restart_tells_the_bot_too() {
    let mut t = interrupted(|_| {}).await;
    t.c.send(json!({"type": "input", "bot_id": t.dev, "data": "\u{4}"}))
        .await;
    // The crash backoff is two seconds.
    let inbox = read_until(&mut t.dev_mcp, "Your session restarted", 160).await;
    assert_eq!(notes(&inbox).len(), 1, "{inbox:?}");
}

#[tokio::test]
async fn resuming_can_be_turned_off() {
    let mut t = interrupted(|cfg| cfg.resume_after_restart = false).await;
    t.c.request(json!({"type": "restart_bot", "bot_id": t.dev}))
        .await;
    let (d, dev) = (&t.d, t.dev.clone());
    wait_until("the bot restarted", || {
        terminal(d, &dev)
            .matches("double runtime ready for dev")
            .count()
            >= 2
    })
    .await;
    let inbox = read_until(&mut t.dev_mcp, "never sent", 20).await;
    assert!(notes(&inbox).is_empty(), "{inbox:?}");
}

#[tokio::test]
async fn a_linked_bot_is_restarted_on_its_machine() {
    let mut t = team().await;
    let restarted = t
        .mac_client
        .request(json!({"type": "restart_bot", "bot_id": t.linked_windev}))
        .await;
    assert_eq!(restarted["type"], "ok", "{restarted}");
    let (win, windev) = (&t.win, t.windev_id.clone());
    wait_until("the PC restarted its bot", || {
        terminal(win, &windev)
            .matches("double runtime ready for windev")
            .count()
            >= 2
    })
    .await;
}
