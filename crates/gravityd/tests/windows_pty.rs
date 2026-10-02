#![cfg(windows)]
use std::time::Duration;

use gravityd::runtime::pty::PtyAdapter;
use gravityd::runtime::{BotSpec, RuntimeAdapter, SessionEvent};

#[tokio::test]
async fn conpty_streams_a_native_process_and_reports_exit() {
    let spec = BotSpec {
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
