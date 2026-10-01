mod common;
mod legacy;

use bus::BotRuntime;
use common::{spawn_daemon, spawn_daemon_with, WsClient};
use gravityd::db::Db;
use serde_json::json;

#[test]
fn migration_defaults_existing_bots_and_persists_runtime() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bus.sqlite");
    let id = legacy::v2_database(&path, "/tmp/bot/workspace");
    let db = Db::open(&path).expect("migrate");
    assert_eq!(
        db.get_bot(&id).unwrap().unwrap().runtime,
        BotRuntime::ClaudeCode
    );
    db.set_bot_runtime(&id, BotRuntime::CodexCli).unwrap();
    drop(db);
    let db = Db::open(&path).unwrap();
    assert_eq!(
        db.get_bot(&id).unwrap().unwrap().runtime,
        BotRuntime::CodexCli
    );
    assert!(db.set_bot_runtime("missing", BotRuntime::CodexCli).is_err());
}

#[tokio::test]
async fn runtime_can_be_created_changed_and_restarted_without_moving_workspace() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let p = c
        .request(json!({"type":"create_project", "name":"Runtime"}))
        .await;
    let b = c.request(json!({"type":"create_bot", "project_id":p["project"]["id"], "name":"Tester", "runtime":"codex_cli"})).await;
    assert_eq!(b["bot"]["runtime"], "codex_cli", "{b}");
    let id = b["bot"]["id"].as_str().unwrap();
    let reply = c
        .request(json!({"type":"set_bot_runtime", "bot_id":id, "runtime":"claude_code"}))
        .await;
    assert_eq!(reply["bot"]["runtime"], "claude_code", "{reply}");
    assert_eq!(reply["bot"]["workspace_path"], b["bot"]["workspace_path"]);
    tokio::time::timeout(std::time::Duration::from_secs(6), async {
        loop {
            if d.app.supervisor.active_total() == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    d.app.supervisor.reconcile();
    assert_eq!(d.app.supervisor.active_total(), 1);
    let invalid = c
        .request(json!({"type":"set_bot_runtime", "bot_id":id, "runtime":"unknown"}))
        .await;
    assert_eq!(invalid["type"], "error");
    assert_eq!(
        d.app.db.get_bot(id).unwrap().unwrap().runtime,
        BotRuntime::ClaudeCode
    );
    let no_op = c
        .request(json!({"type":"set_bot_runtime", "bot_id":id, "runtime":"claude_code"}))
        .await;
    assert_eq!(no_op["type"], "bot");
    assert_eq!(d.app.supervisor.active_total(), 1);
}

#[tokio::test]
async fn omitted_runtime_uses_configured_default() {
    let d = spawn_daemon_with(|cfg| cfg.default_bot_runtime = BotRuntime::CodexCli).await;
    let mut c = WsClient::connect(&d).await;
    let p = c
        .request(json!({"type":"create_project", "name":"Default"}))
        .await;
    let b = common::create_bot(&mut c, p["project"]["id"].as_str().unwrap(), "Default").await;
    assert_eq!(b["runtime"], "codex_cli");
}
