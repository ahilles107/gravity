//! Opt-in tests of the installed Codex CLI; model turns use only a local fixture.
use gravityd::runtime::codex::{CodexAdapter, CodexSpec, NativeCodexAdapter};
use gravityd::runtime::{BotSpec, RuntimeAdapter, SessionEvent};

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
            browser: None,
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

#[tokio::test]
#[ignore = "requires CODEX_BINARY; native terminal with local mock Responses"]
async fn native_terminal_renders_the_official_codex_interface() {
    use std::io::BufRead;
    let mut provider = LocalProvider(
        std::process::Command::new(std::env::var("NODE_BINARY").unwrap_or_else(|_| "node".into()))
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/codex-responses.mjs"),
            )
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut endpoint = String::new();
    std::io::BufReader::new(provider.0.stdout.take().unwrap())
        .read_line(&mut endpoint)
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let home = root.path().join("codex-home");
    std::fs::create_dir_all(&home).unwrap();
    let spec = BotSpec {
        bot_id: "native-tui".into(),
        bot_name: "Native TUI".into(),
        workspace,
        claude_bin: "unused".into(),
        claude_args: Vec::new(),
        codex: Some(CodexSpec {
            bin: std::env::var("CODEX_BINARY").unwrap(),
            args: vec!["-c".into(), "model_provider=\"gravity_test\"".into(), "-c".into(), format!("model_providers.gravity_test={{name=\"Local test\",base_url=\"{}\",wire_api=\"responses\",requires_openai_auth=false}}", endpoint.trim())],
            port: 1,
            artifacts: None,
            browser: None,
        }),
        env: vec![("CODEX_HOME".into(), home.display().to_string()), ("NO_PROXY".into(), "127.0.0.1,localhost".into())],
        cols: 100,
        rows: 30,
    };
    let mut started = NativeCodexAdapter.start(&spec).unwrap();
    let mut output = String::new();
    let rendered = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            match started.events.recv().await.unwrap() {
                SessionEvent::Output(bytes) => {
                    output.push_str(&String::from_utf8_lossy(&bytes));
                    if bytes.windows(4).any(|chunk| chunk == b"\x1b[6n") {
                        started.session.send_input(b"\x1b[1;1R").unwrap();
                    }
                    if output.contains("OpenAI Codex") && output.contains("/review") {
                        break;
                    }
                }
                SessionEvent::Exited { code } => panic!("native TUI exited {code:?}: {output}"),
                _ => {}
            }
        }
    })
    .await;
    if rendered.is_ok() {
        started
            .session
            .deliver("native bus smoke")
            .unwrap()
            .unwrap();
        let reply = tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                match started.events.recv().await.unwrap() {
                    SessionEvent::Output(bytes) => {
                        output.push_str(&String::from_utf8_lossy(&bytes))
                    }
                    SessionEvent::Lifecycle {
                        event: "Stop" | "TurnInterrupted",
                        ..
                    } => break,
                    SessionEvent::Exited { code } => panic!("native exit {code:?}: {output}"),
                    _ => {}
                }
            }
        })
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        while let Ok(SessionEvent::Output(bytes)) = started.events.try_recv() {
            output.push_str(&String::from_utf8_lossy(&bytes));
        }
        assert!(reply.is_ok(), "local response timeout: {output}");
        started
            .session
            .send_input(b"native keyboard smoke")
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        started.session.send_input(b"\r").unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                match started.events.recv().await.unwrap() {
                    SessionEvent::Lifecycle { event: "Stop", .. } => break,
                    SessionEvent::Exited { code } => panic!("native exit {code:?}"),
                    SessionEvent::Output(bytes) => {
                        output.push_str(&String::from_utf8_lossy(&bytes))
                    }
                    _ => {}
                }
            }
        })
        .await
        .unwrap_or_else(|error| panic!("native keyboard turn: {error}: {output}"));
        started.session.kill().unwrap();
        let transcript =
            std::fs::read_to_string(root.path().join("codex-observations.jsonl")).unwrap();
        assert!(
            transcript.contains("Local response: native bus smoke"),
            "{transcript}\n{output}"
        );
        assert!(
            transcript.contains("Local response: native keyboard smoke"),
            "{transcript}"
        );
        drop(started);
        let mut resumed = NativeCodexAdapter
            .start(&spec)
            .expect("persisted native conversation resumes");
        resumed.session.kill().unwrap();
    } else {
        started.session.kill().unwrap();
    }
    assert!(rendered.is_ok(), "native interface timed out: {output}");
    assert!(!output.contains("Codex CLI ready. Type a prompt"));
}

struct LocalProvider(std::process::Child);
impl Drop for LocalProvider {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
