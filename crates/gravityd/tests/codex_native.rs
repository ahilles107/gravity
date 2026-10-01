//! Opt-in handshake against a locally installed Codex CLI; no model turn is sent.
use gravityd::runtime::codex::{CodexAdapter, CodexSpec};
use gravityd::runtime::{BotSpec, RuntimeAdapter};

#[test]
#[ignore = "requires CODEX_BINARY pointing to a locally installed CLI"]
fn native_codex_initializes_and_resumes_without_a_model_request() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(root.path().join("codex-home")).unwrap();
    let spec = BotSpec {
        bot_id: "native-smoke".into(),
        bot_name: "Native smoke".into(),
        workspace,
        claude_bin: "unused".into(),
        claude_args: Vec::new(),
        codex: Some(CodexSpec {
            bin: std::env::var("CODEX_BINARY").expect("CODEX_BINARY"),
            args: Vec::new(),
            port: 1,
            artifacts: None,
        }),
        env: vec![
            ("GRAVITY_TOKEN".into(), "smoke-not-a-real-token".into()),
            (
                "CODEX_HOME".into(),
                root.path().join("codex-home").display().to_string(),
            ),
        ],
        cols: 80,
        rows: 24,
    };
    let mut started = CodexAdapter.start(&spec).expect("native initialization");
    assert!(root.path().join("codex-thread-id").is_file());
    started.session.kill().unwrap();
    drop(started);
    let mut resumed = CodexAdapter.start(&spec).expect("native thread resume");
    resumed.session.kill().unwrap();
}
