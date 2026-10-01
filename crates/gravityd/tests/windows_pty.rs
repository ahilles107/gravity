#![cfg(windows)]
mod common;
use std::time::Duration;

use gravityd::runtime::pty::PtyAdapter;
use gravityd::runtime::{BotSpec, RuntimeAdapter, SessionEvent};
use serde_json::json;

#[tokio::test]
async fn conpty_streams_a_native_process_and_reports_exit() {
    let spec = BotSpec {
        codex: None,
        bot_id: "windows-pty".into(),
        bot_name: "PTY".into(),
        workspace: std::env::temp_dir(),
        claude_bin: "powershell.exe".into(),
        claude_args: vec![
            "-NoProfile".into(),
            "-Command".into(),
            "Write-Output 'gravity-conpty-ok'".into(),
        ],
        env: Vec::new(),
        cols: 80,
        rows: 24,
    };
    let mut started = PtyAdapter.start(&spec).expect("native ConPTY");
    let collect = async {
        let mut bytes = Vec::new();
        while let Some(event) = started.events.recv().await {
            match event {
                SessionEvent::Output(data) => bytes.extend(data),
                SessionEvent::Exited { code } => {
                    assert_eq!(code, Some(0));
                    return bytes;
                }
                SessionEvent::Lifecycle { .. } => {}
            }
        }
        panic!("process must report exit");
    };
    let bytes = tokio::time::timeout(Duration::from_secs(15), collect)
        .await
        .expect("PTY exit");
    assert!(String::from_utf8_lossy(&bytes).contains("gravity-conpty-ok"));
    started
        .session
        .kill()
        .expect("stopping an exited process is harmless");
}

#[tokio::test]
async fn conpty_kills_a_live_process_without_reporting_a_stale_handle_error() {
    let spec = BotSpec {
        codex: None,
        bot_id: "windows-kill".into(),
        bot_name: "Kill".into(),
        workspace: std::env::temp_dir(),
        claude_bin: "powershell.exe".into(),
        claude_args: vec![
            "-NoProfile".into(),
            "-Command".into(),
            "Write-Output 'waiting-for-switch'; Start-Sleep -Seconds 120".into(),
        ],
        env: Vec::new(),
        cols: 80,
        rows: 24,
    };
    let mut started = PtyAdapter.start(&spec).expect("native ConPTY");
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Some(SessionEvent::Output(bytes)) = started.events.recv().await {
                if String::from_utf8_lossy(&bytes).contains("waiting-for-switch") {
                    break;
                }
            }
        }
    })
    .await
    .expect("live child output");
    started
        .session
        .kill()
        .expect("terminate a live process successfully");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(SessionEvent::Exited { .. }) = started.events.recv().await {
                break;
            }
        }
    })
    .await
    .expect("killed child reports exit");
    started.session.kill().expect("repeated stop is harmless");
}

#[tokio::test]
async fn control_plane_switches_a_live_conpty_to_codex_and_back() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let daemon = common::spawn_daemon_with(|cfg| {
        cfg.runtime = gravityd::config::RuntimeKind::Pty;
        cfg.supervision_interval_ms = 50;
        cfg.user_home = cfg.home.join("cli-home");
        std::fs::create_dir_all(&cfg.user_home).unwrap();
        cfg.claude_bin = "node".into();
        cfg.claude_args = vec![fixtures.join("live-pty.mjs").display().to_string()];
        cfg.codex_bin = "node".into();
        cfg.codex_args = vec![fixtures.join("codex-server.mjs").display().to_string()];
    })
    .await;
    let mut client = common::WsClient::connect(&daemon).await;
    let project = client
        .request(json!({"type":"create_project", "name":"Switch"}))
        .await;
    let bot = common::create_bot(
        &mut client,
        project["project"]["id"].as_str().unwrap(),
        "Native",
    )
    .await;
    let id = bot["id"].as_str().unwrap();
    let root = std::path::Path::new(bot["workspace_path"].as_str().unwrap())
        .parent()
        .unwrap();
    let wait = async {
        while !root.join("pty-launches").is_file()
            || daemon.app.supervisor.state(id).0 != bus::BotState::Ready
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .unwrap();
    let reply = client
        .request(json!({"type":"set_bot_runtime", "bot_id":id, "runtime":"codex_cli"}))
        .await;
    assert_eq!(reply["bot"]["runtime"], "codex_cli", "{reply}");
    tokio::time::timeout(Duration::from_secs(10), async {
        while !root.join("codex-thread-id").is_file()
            || daemon.app.supervisor.state(id).0 != bus::BotState::Ready
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Codex starts after terminating the native PTY");
    let reply = client
        .request(json!({"type":"set_bot_runtime", "bot_id":id, "runtime":"claude_code"}))
        .await;
    assert_eq!(reply["bot"]["runtime"], "claude_code", "{reply}");
    tokio::time::timeout(Duration::from_secs(10), async {
        while !std::fs::read_to_string(root.join("pty-launches"))
            .is_ok_and(|text| text.lines().count() >= 2)
            || daemon.app.supervisor.state(id).0 != bus::BotState::Ready
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("native PTY starts again");
    daemon.app.supervisor.stop_bot(id).unwrap();
}
