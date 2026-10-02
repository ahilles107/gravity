use gravityd::runtime::codex::{CodexAdapter, CodexSpec, NativeCodexAdapter};
use gravityd::runtime::{BotSpec, PermissionAnswer, RuntimeAdapter, SessionEvent, StartedSession};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

fn spec(root: &Path) -> BotSpec {
    let workspace = root.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(root.join("system.md"), "Shared system instructions").unwrap();
    BotSpec {
        bot_id: "test".into(),
        bot_name: "Test".into(),
        workspace,
        claude_bin: "unused".into(),
        claude_args: Vec::new(),
        codex: Some(CodexSpec {
            bin: std::env::var("NODE_BINARY").unwrap_or_else(|_| "node".into()),
            args: vec![Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/codex-server.mjs")
                .display()
                .to_string()],
            port: 49777,
            artifacts: Some(root.join("artifacts")),
        }),
        env: vec![
            ("GRAVITY_TOKEN".into(), "test-token".into()),
            (
                "GRAVITY_TEST_LOG".into(),
                root.join("rpc.jsonl").display().to_string(),
            ),
        ],
        cols: 80,
        rows: 24,
    }
}

#[tokio::test]
async fn native_terminal_and_bus_share_history_without_consuming_a_draft() {
    let root = tempfile::tempdir().unwrap();
    let spec = spec(root.path());
    let mut started = NativeCodexAdapter.start(&spec).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match started.events.recv().await.unwrap() {
                SessionEvent::Output(bytes)
                    if String::from_utf8_lossy(&bytes).contains("Native Codex CLI ready") =>
                {
                    break
                }
                SessionEvent::Exited { code } => panic!("native terminal exit {code:?}"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    started.session.resize(120, 40).unwrap();
    started.session.send_input("draft 🪟".as_bytes()).unwrap();
    started.session.deliver("bus envelope").unwrap().unwrap();
    hook(&mut started, "Stop").await;
    started.session.send_input(b"\r").unwrap();
    hook(&mut started, "Stop").await;
    let turns: Vec<_> = log(root.path())
        .into_iter()
        .filter(|v| v["method"] == "turn/start")
        .collect();
    assert_eq!(turns[0]["params"]["input"][0]["text"], "bus envelope");
    assert_eq!(turns[1]["params"]["input"][0]["text"], "draft 🪟");
    let observations =
        std::fs::read_to_string(root.path().join("codex-observations.jsonl")).unwrap();
    assert_eq!(observations.matches("bus envelope").count(), 1);
    assert!(observations.contains("draft 🪟"));
    started.session.deliver("approval").unwrap().unwrap();
    hook(&mut started, "Notification").await;
    assert!(!log(root.path())
        .iter()
        .any(|v| v["id"] == "approval-1" && v.get("result").is_some()));
    started.session.send_input(b"n").unwrap();
    hook(&mut started, "Stop").await;
    assert!(log(root.path())
        .iter()
        .any(|v| v["id"] == "approval-1" && v["result"]["decision"] == "decline"));
    started.session.send_input(b"/exit\r").unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(SessionEvent::Exited { .. }) = started.events.recv().await {
                break;
            }
        }
    })
    .await
    .unwrap();
    started.session.kill().unwrap();
    let mut resumed = NativeCodexAdapter.start(&spec).unwrap();
    assert!(log(root.path())
        .iter()
        .any(|v| v["method"] == "thread/resume"));
    resumed.session.kill().unwrap();
}

fn log(root: &Path) -> Vec<Value> {
    std::fs::read_to_string(root.join("rpc.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

async fn hook(started: &mut StartedSession, name: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match started.events.recv().await.unwrap() {
                SessionEvent::Lifecycle { event, .. } if event == name => break,
                SessionEvent::Exited { code } => panic!("unexpected exit {code:?}"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn structured_delivery_does_not_consume_terminal_draft_and_resumes_history() {
    let root = tempfile::tempdir().unwrap();
    let spec = spec(root.path());
    let mut started = CodexAdapter.start(&spec).unwrap();
    hook(&mut started, "SessionStart").await;
    started.session.send_input("draft 🪟".as_bytes()).unwrap();
    started.session.deliver("bus envelope").unwrap().unwrap();
    hook(&mut started, "Stop").await;
    started.session.send_input(b"\r").unwrap();
    hook(&mut started, "Stop").await;
    let turns: Vec<_> = log(root.path())
        .into_iter()
        .filter(|v| v["method"] == "turn/start")
        .collect();
    assert_eq!(turns[0]["params"]["input"][0]["text"], "bus envelope");
    assert_eq!(turns[1]["params"]["input"][0]["text"], "draft 🪟");
    let observed = std::fs::read_to_string(root.path().join("codex-observations.jsonl")).unwrap();
    assert!(observed.contains("Hello 🪟"));
    started.session.kill().unwrap();
    drop(started);
    let mut resumed = CodexAdapter.start(&spec).unwrap();
    hook(&mut resumed, "SessionStart").await;
    assert!(log(root.path())
        .iter()
        .any(|v| v["method"] == "thread/resume" && v["params"]["threadId"] == "thread-fixture"));
    resumed.session.kill().unwrap();
}

#[tokio::test]
async fn approvals_are_explicit_and_interruption_never_reports_success() {
    let root = tempfile::tempdir().unwrap();
    let mut started = CodexAdapter.start(&spec(root.path())).unwrap();
    started.session.deliver("approval").unwrap().unwrap();
    hook(&mut started, "Notification").await;
    assert!(!log(root.path())
        .iter()
        .any(|v| v["id"] == "approval-1" && v.get("result").is_some()));
    started.session.send_input(b"/deny 1\r").unwrap();
    hook(&mut started, "Stop").await;
    assert!(log(root.path())
        .iter()
        .any(|v| v["id"] == "approval-1" && v["result"]["decision"] == "decline"));
    started.session.deliver("hold").unwrap().unwrap();
    started.session.deliver("steered message").unwrap().unwrap();
    assert!(log(root.path())
        .iter()
        .any(|v| v["method"] == "turn/steer" && v["params"]["expectedTurnId"] == "turn-fixture"));
    started.session.send_input(b"\x03").unwrap();
    hook(&mut started, "TurnInterrupted").await;
    started.session.deliver("crash").unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let SessionEvent::Exited { code } = started.events.recv().await.unwrap() {
                assert_eq!(code, Some(7));
                break;
            }
        }
    })
    .await
    .unwrap();
}

/// The next permission request the session raises, and its key.
async fn permission(started: &mut StartedSession) -> (u64, String, Value) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let SessionEvent::Permission { key, tool, input } =
                started.events.recv().await.unwrap()
            {
                return (key, tool, input);
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn a_card_answers_a_native_approval_for_the_session() {
    let root = tempfile::tempdir().unwrap();
    let mut started = NativeCodexAdapter.start(&spec(root.path())).unwrap();
    started.session.deliver("approval").unwrap().unwrap();
    let (key, tool, input) = permission(&mut started).await;
    assert_eq!(tool, "Bash");
    assert_eq!(input["command"], "echo hello");
    started
        .session
        .answer_permission(key, PermissionAnswer::Session)
        .unwrap();
    hook(&mut started, "Stop").await;
    assert!(log(root.path())
        .iter()
        .any(|v| v["id"] == "approval-1" && v["result"]["decision"] == "acceptForSession"));
    started.session.kill().unwrap();
}

#[tokio::test]
async fn an_approval_answered_in_the_terminal_withdraws_its_card() {
    let root = tempfile::tempdir().unwrap();
    let mut started = CodexAdapter.start(&spec(root.path())).unwrap();
    started.session.deliver("approval").unwrap().unwrap();
    let (key, _, _) = permission(&mut started).await;
    started.session.send_input(b"/deny 1\r").unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let SessionEvent::PermissionGone { key: gone } = started.events.recv().await.unwrap()
            {
                assert_eq!(gone, key);
                break;
            }
        }
    })
    .await
    .unwrap();
    // Too late: the request is gone, and the card's answer is not sent.
    started
        .session
        .answer_permission(key, PermissionAnswer::Once)
        .unwrap();
    hook(&mut started, "Stop").await;
    assert!(!log(root.path())
        .iter()
        .any(|v| v["id"] == "approval-1" && v["result"]["decision"] == "accept"));
    started.session.kill().unwrap();
}

#[tokio::test]
async fn tool_items_are_recorded_for_the_chat() {
    let root = tempfile::tempdir().unwrap();
    let mut started = CodexAdapter.start(&spec(root.path())).unwrap();
    started.session.deliver("tools").unwrap().unwrap();
    hook(&mut started, "Stop").await;
    let records: Vec<Value> = std::fs::read_to_string(root.path().join("codex-observations.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let tools: Vec<&str> = records
        .iter()
        .filter_map(|r| r["message"]["content"][0]["name"].as_str())
        .collect();
    assert_eq!(tools, ["Bash", "Edit", "mcp__gravity-bus__send_message"]);
    let patch = records
        .iter()
        .find_map(|r| r["toolUseResult"]["structuredPatch"].as_array())
        .expect("the edit's patch");
    assert_eq!(
        patch[0]["lines"],
        serde_json::json!(["-old", "+new", "+more"])
    );
    assert!(records
        .iter()
        .any(|r| r["type"] == "system" && r["subtype"] == "turn_duration"));
    started.session.kill().unwrap();
}
