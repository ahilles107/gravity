#![cfg(windows)]
use std::time::Duration;

use gravityd::runtime::pty::PtyAdapter;
use gravityd::runtime::{BotSpec, RuntimeAdapter, SessionEvent};

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
}
