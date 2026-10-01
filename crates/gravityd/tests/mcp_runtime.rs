//! Runtime selection through the real bot-authenticated MCP endpoint.

mod common;

use bus::BotRuntime;
use common::{spawn_daemon, spawn_daemon_with, McpClient, TestDaemon, WsClient};
use gravityd::config::RuntimeKind;
use serde_json::{json, Value};

async fn lead(d: &TestDaemon, runtime: BotRuntime) -> (Value, McpClient) {
    let mut client = WsClient::connect(d).await;
    let project = client
        .request(json!({"type":"create_project", "name":"Team"}))
        .await;
    let reply = client
        .request(json!({
            "type":"create_bot", "project_id":project["project"]["id"],
            "name":"Lead", "runtime":runtime
        }))
        .await;
    assert_eq!(reply["type"], "bot", "{reply}");
    let bot = reply["bot"].clone();
    let token = d
        .app
        .secrets
        .bot_token(bot["id"].as_str().unwrap())
        .unwrap();
    (bot, McpClient::new(d, &token))
}

#[tokio::test]
async fn schemas_advertise_optional_runtime_and_results_verify_it() {
    let d = spawn_daemon().await;
    let (_, mut bus) = lead(&d, BotRuntime::CodexCli).await;
    let manifest = bus.tools().await;
    for name in ["create_bot", "update_bot"] {
        let tool = manifest["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap();
        assert_eq!(
            tool["inputSchema"]["properties"]["runtime"]["enum"],
            json!(["claude_code", "codex_cli"])
        );
        assert_eq!(tool["inputSchema"]["required"], json!(["name"]));
    }
    assert_eq!(
        bus.call("get_self", json!({})).await["runtime"],
        "codex_cli"
    );
    let created = bus
        .call(
            "create_bot",
            json!({"name":"Claude", "runtime":"claude_code"}),
        )
        .await;
    assert_eq!(created["runtime"], "claude_code");
    let listed = bus.call("list_bots", json!({})).await;
    let child = listed["bots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|bot| bot["name"] == "Claude")
        .unwrap();
    assert_eq!(child["runtime"], "claude_code");
    let token = d
        .app
        .secrets
        .bot_token(child["id"].as_str().unwrap())
        .unwrap();
    let mut child_bus = McpClient::new(&d, &token);
    assert_eq!(
        child_bus.call("get_self", json!({})).await["runtime"],
        "claude_code"
    );
}

#[tokio::test]
async fn children_inherit_or_override_both_providers_independently_of_default() {
    for (parent, other) in [
        (BotRuntime::ClaudeCode, BotRuntime::CodexCli),
        (BotRuntime::CodexCli, BotRuntime::ClaudeCode),
    ] {
        let d = spawn_daemon_with(|cfg| cfg.default_bot_runtime = other).await;
        let (lead, mut bus) = lead(&d, parent).await;
        assert_eq!(
            bus.call("create_bot", json!({"name":"Inherited"})).await["runtime"],
            json!(parent)
        );
        assert_eq!(
            bus.call("create_bot", json!({"name":"Override", "runtime":other}))
                .await["runtime"],
            json!(other)
        );
        let project_id = lead["project_id"].as_str().unwrap();
        assert_eq!(
            d.app
                .db
                .get_bot_by_name(project_id, "Inherited")
                .unwrap()
                .unwrap()
                .runtime,
            parent
        );
        assert_eq!(
            d.app
                .db
                .get_bot_by_name(project_id, "Override")
                .unwrap()
                .unwrap()
                .runtime,
            other
        );
    }
}

#[tokio::test]
async fn child_runtime_updates_restart_and_preserve_workspace_and_omitted_choice() {
    let d = spawn_daemon().await;
    let (lead, mut bus) = lead(&d, BotRuntime::ClaudeCode).await;
    bus.call("create_bot", json!({"name":"Helper"})).await;
    let pid = lead["project_id"].as_str().unwrap();
    let before = d.app.db.get_bot_by_name(pid, "Helper").unwrap().unwrap();
    let updated = bus
        .call(
            "update_bot",
            json!({"name":"Helper", "runtime":"codex_cli"}),
        )
        .await;
    assert_eq!(updated["runtime"], "codex_cli");
    assert!(updated["note"].as_str().unwrap().contains("restarting"));
    tokio::time::timeout(std::time::Duration::from_secs(6), async {
        loop {
            if d.app.supervisor.active_total() == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    d.app.supervisor.reconcile();
    assert_eq!(d.app.supervisor.active_total(), 2);
    let after = d.app.db.get_bot(&before.id).unwrap().unwrap();
    assert_eq!(after.runtime, BotRuntime::CodexCli);
    assert_eq!(after.workspace_path, before.workspace_path);
    for args in [
        json!({"name":"Helper", "runtime":"codex_cli"}),
        json!({"name":"Helper", "description":"changed"}),
    ] {
        assert_eq!(bus.call("update_bot", args).await["runtime"], "codex_cli");
        assert_eq!(
            d.app.supervisor.active_total(),
            2,
            "same/omitted runtime must not restart"
        );
    }
}

#[tokio::test]
async fn invalid_choices_do_not_create_bots_or_change_identity() {
    let d = spawn_daemon().await;
    let (lead, mut bus) = lead(&d, BotRuntime::ClaudeCode).await;
    bus.call(
        "create_bot",
        json!({"name":"Helper", "description":"original"}),
    )
    .await;
    let pid = lead["project_id"].as_str().unwrap();
    for runtime in [json!("unknown"), json!(null), json!(42)] {
        let error = bus
            .call_raw("create_bot", json!({"name":"Invalid", "runtime":runtime}))
            .await;
        assert_eq!(error["isError"], true);
        assert!(d.app.db.get_bot_by_name(pid, "Invalid").unwrap().is_none());
        let error = bus
            .call_raw(
                "update_bot",
                json!({"name":"Helper", "runtime":runtime, "description":"bad"}),
            )
            .await;
        assert_eq!(error["isError"], true);
        let child = d.app.db.get_bot_by_name(pid, "Helper").unwrap().unwrap();
        assert_eq!(child.runtime, BotRuntime::ClaudeCode);
        assert_eq!(child.description, "original");
    }
}

#[tokio::test]
async fn runtime_changes_require_direct_parentage() {
    let d = spawn_daemon().await;
    let (lead, mut bus) = lead(&d, BotRuntime::ClaudeCode).await;
    bus.call("create_bot", json!({"name":"Child"})).await;
    let pid = lead["project_id"].as_str().unwrap();
    let child = d.app.db.get_bot_by_name(pid, "Child").unwrap().unwrap();
    let token = d.app.secrets.bot_token(&child.id).unwrap();
    let mut child_bus = McpClient::new(&d, &token);
    child_bus
        .call("create_bot", json!({"name":"Grandchild"}))
        .await;
    let mut ws = WsClient::connect(&d).await;
    common::create_bot(&mut ws, pid, "Peer").await;
    for name in ["Lead", "Peer", "Grandchild"] {
        let error = bus
            .call_raw("update_bot", json!({"name":name, "runtime":"codex_cli"}))
            .await;
        assert_eq!(error["isError"], true, "{error}");
        assert_eq!(
            d.app
                .db
                .get_bot_by_name(pid, name)
                .unwrap()
                .unwrap()
                .runtime,
            BotRuntime::ClaudeCode
        );
    }
}

#[tokio::test]
async fn unavailable_cli_rejects_creation_and_changes_before_mutation() {
    let d = spawn_daemon_with(|cfg| {
        cfg.runtime = RuntimeKind::Pty;
        cfg.user_home = cfg.home.join("user");
        cfg.claude_bin = cfg.home.join("missing-claude.exe").display().to_string();
        cfg.codex_bin = cfg.home.join("missing-codex.exe").display().to_string();
    })
    .await;
    let (lead, mut bus) = lead(&d, BotRuntime::ClaudeCode).await;
    let pid = lead["project_id"].as_str().unwrap();
    let parent_id = lead["id"].as_str().unwrap();
    let workspace = d.app.cfg.home.join("unused-workspace");
    let child = d
        .app
        .db
        .create_bot_with_runtime(
            pid,
            "Helper",
            "original",
            "help",
            "icon:nova",
            &workspace.display().to_string(),
            "helper",
            Some(parent_id),
            BotRuntime::ClaudeCode,
        )
        .unwrap();
    for runtime in ["claude_code", "codex_cli"] {
        let error = bus
            .call_raw(
                "create_bot",
                json!({"name":"Unavailable", "runtime":runtime}),
            )
            .await;
        assert_eq!(error["isError"], true, "{error}");
        assert!(d
            .app
            .db
            .get_bot_by_name(pid, "Unavailable")
            .unwrap()
            .is_none());
    }
    let error = bus
        .call_raw(
            "update_bot",
            json!({"name":"Helper", "runtime":"codex_cli", "description":"bad"}),
        )
        .await;
    assert_eq!(error["isError"], true, "{error}");
    let after = d.app.db.get_bot(&child.id).unwrap().unwrap();
    assert_eq!(after.runtime, BotRuntime::ClaudeCode);
    assert_eq!(after.description, "original");
    let mut ws = WsClient::connect(&d).await;
    let error = ws
        .request(json!({"type":"set_bot_runtime", "bot_id":child.id, "runtime":"codex_cli"}))
        .await;
    assert_eq!(error["code"], "runtime_unavailable", "{error}");
}
