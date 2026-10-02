#![cfg(windows)]
//! Opt-in native CLI startup check, isolated from account data and model traffic.
use gravityd::runtime::pty::PtyAdapter;
use gravityd::runtime::{BotSpec, RuntimeAdapter, SessionEvent};
use serde_json::json;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires CLAUDE_BINARY pointing to a locally installed CLI"]
async fn native_claude_reaches_its_prompt_without_asking_for_workspace_trust() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    let config = root.path().join("config");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(&config).unwrap();
    let fake_key = "sk-ant-gravity-smoke-local-only";
    std::fs::write(config.join(".claude.json"), serde_json::to_vec(&json!({
        "hasCompletedOnboarding": true, "lastOnboardingVersion": "2.1.286", "numStartups": 1, "theme": "dark",
        "customApiKeyResponses": {"approved": [fake_key.chars().skip(fake_key.len() - 20).collect::<String>()], "rejected": []}
    })).unwrap()).unwrap();
    gravityd::paths::trust_workspace(&config, &workspace).unwrap();
    let spec = BotSpec {
        codex: None,
        bot_id: "native-trust".into(),
        bot_name: "Trust check".into(),
        workspace,
        claude_bin: std::env::var("CLAUDE_BINARY").expect("CLAUDE_BINARY"),
        claude_args: vec![
            "--strict-mcp-config".into(),
            "--mcp-config".into(),
            "{\"mcpServers\":{}}".into(),
        ],
        env: vec![
            ("CLAUDE_CONFIG_DIR".into(), config.display().to_string()),
            ("ANTHROPIC_API_KEY".into(), fake_key.into()),
            ("ANTHROPIC_BASE_URL".into(), "http://127.0.0.1:1".into()),
            (
                "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(),
                "1".into(),
            ),
            ("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN".into(), "1".into()),
        ],
        cols: 120,
        rows: 40,
    };
    let mut started = PtyAdapter.start(&spec).expect("native Claude process");
    let mut output = String::new();
    let ready = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(event) = started.events.recv().await {
            match event {
                SessionEvent::Output(bytes) => {
                    output.push_str(&String::from_utf8_lossy(&bytes));
                    if output.contains("shift+tab") {
                        return true;
                    }
                }
                SessionEvent::Exited { .. } => return false,
                SessionEvent::Lifecycle { .. } => {}
            }
        }
        false
    })
    .await;
    started.session.kill().unwrap();
    assert!(
        matches!(ready, Ok(true)),
        "Claude did not reach its prompt: {output}"
    );
    assert!(
        !output.to_ascii_lowercase().contains("trust"),
        "unexpected trust request: {output}"
    );
}
