use gravityd::runtime::codex::{CodexAdapter, CodexSpec, NativeCodexAdapter};
use gravityd::runtime::{BotSpec, RuntimeAdapter, SessionEvent, StartedSession};
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
