//! Each bot's own browser: the owner's Chrome is opt-in per bot, the session
//! is configured with a browser of its own, and the app can watch it live.
//! A fake DevTools endpoint stands in for Chrome, so no browser is needed.

mod common;

use std::time::Duration;

use common::peers::{project, wait_until};
use common::*;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as WsMsg;

fn bot_root(d: &TestDaemon, bot_id: &str) -> std::path::PathBuf {
    let bot = d.app.db.get_bot(bot_id).expect("db").expect("bot");
    std::path::PathBuf::from(&bot.workspace_path)
        .parent()
        .expect("root")
        .to_path_buf()
}

#[tokio::test]
async fn bots_use_their_own_browser_unless_allowed_the_owners_chrome() {
    let d = spawn_daemon_with(|cfg| {
        cfg.user_home = cfg.home.join("user");
        cfg.browser.node_dir = Some(cfg.home.join("node"));
    })
    .await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "web").await;
    let bot = create_bot(&mut c, &pid, "surfer").await;
    let id = bot["id"].as_str().expect("id").to_string();
    assert_eq!(bot["user_chrome"], false);

    wait_until("the bot starts without the owner's Chrome", || {
        terminal(&d, &id).contains("--no-chrome")
    })
    .await;
    let mcp: Value = serde_json::from_str(
        &std::fs::read_to_string(bot_root(&d, &id).join("mcp.json")).expect("mcp.json"),
    )
    .expect("json");
    let browser = &mcp["mcpServers"]["playwright"];
    assert_eq!(browser["type"], "stdio", "{mcp}");
    assert!(browser["args"]
        .as_array()
        .expect("args")
        .iter()
        .any(|a| a.as_str().is_some_and(|a| a.ends_with("playwright.json"))));

    let allowed = c
        .request(json!({"type": "set_bot_user_chrome", "bot_id": id, "enabled": true}))
        .await;
    assert_eq!(allowed["type"], "bot", "{allowed}");
    assert_eq!(allowed["bot"]["user_chrome"], true);
    wait_until("the restarted session may use the owner's Chrome", || {
        terminal(&d, &id).matches("--chrome ").count() >= 1
    })
    .await;
    let system = std::fs::read_to_string(bot_root(&d, &id).join("system.md")).expect("system");
    assert!(system.contains("claude-in-chrome"));
}

/// Serves `/json/list` with one page, and that page's WebSocket answering
/// `Page.startScreencast` with a frame. Returns the HTTP port.
async fn fake_devtools(title: &'static str) -> u16 {
    let ws = TcpListener::bind("127.0.0.1:0").await.expect("bind ws");
    let ws_port = ws.local_addr().expect("addr").port();
    tokio::spawn(async move {
        while let Ok((stream, _)) = ws.accept().await {
            tokio::spawn(async move {
                let mut socket = tokio_tungstenite::accept_async(stream).await.expect("ws");
                while let Some(Ok(WsMsg::Text(text))) = socket.next().await {
                    let call: Value = serde_json::from_str(&text).expect("json");
                    if call["method"] == "Page.startScreencast" {
                        let frame = json!({
                            "method": "Page.screencastFrame",
                            "params": { "data": "SlBFRw==", "sessionId": 1,
                                        "metadata": { "deviceWidth": 800, "deviceHeight": 600 } }
                        });
                        let _ = socket.send(WsMsg::Text(frame.to_string())).await;
                    }
                }
            });
        }
    });
    let http = TcpListener::bind("127.0.0.1:0").await.expect("bind http");
    let port = http.local_addr().expect("addr").port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = http.accept().await {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
            let body = json!([{
                "id": "tab-1", "type": "page", "title": title, "url": "https://example.com/",
                "webSocketDebuggerUrl": format!("ws://127.0.0.1:{ws_port}/devtools/page/tab-1")
            }])
            .to_string();
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes()).await;
        }
    });
    port
}

#[tokio::test]
async fn the_app_watches_a_bots_browser_live() {
    let d = spawn_daemon_with(|cfg| cfg.user_home = cfg.home.join("user")).await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "web").await;
    let bot = create_bot(&mut c, &pid, "surfer").await;
    let id = bot["id"].as_str().expect("id").to_string();

    // No browser yet: the watch says so, and keeps watching.
    let ok = c
        .request(json!({"type": "watch_browser", "bot_id": id}))
        .await;
    assert_eq!(ok["type"], "ok", "{ok}");
    let closed = c.wait_for(|v| v["type"] == "browser_tabs").await;
    assert_eq!(closed["open"], false);

    // The bot's browser comes up and writes its DevTools port to its profile.
    let port = fake_devtools("Example").await;
    let profile = bot_root(&d, &id).join("browser/profile");
    std::fs::create_dir_all(&profile).expect("mkdir");
    std::fs::write(
        profile.join("DevToolsActivePort"),
        format!("{port}\n/devtools/browser/x"),
    )
    .expect("port");

    let tabs = c
        .wait_for(|v| v["type"] == "browser_tabs" && v["open"] == true)
        .await;
    assert_eq!(tabs["tabs"][0]["title"], "Example");
    assert_eq!(tabs["active"], "tab-1");
    assert_eq!(tabs["following"], true);
    let frame = c.wait_for(|v| v["type"] == "browser_frame").await;
    assert_eq!(frame["bot_id"], id.as_str());
    assert_eq!(frame["tab_id"], "tab-1");
    assert_eq!(frame["data"], "SlBFRw==");
    assert_eq!(
        (frame["width"].as_u64(), frame["height"].as_u64()),
        (Some(800), Some(600))
    );

    let stopped = c.request(json!({"type": "unwatch_browser"})).await;
    assert_eq!(stopped["type"], "ok");
    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[tokio::test]
async fn the_browser_log_says_what_each_step_did_and_why() {
    let d = spawn_daemon_with(|cfg| cfg.user_home = cfg.home.join("user")).await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "web").await;
    let bot = create_bot(&mut c, &pid, "surfer").await;
    let id = bot["id"].as_str().expect("id").to_string();
    let workspace = bot["workspace_path"].as_str().expect("workspace");
    let mangled: String = workspace
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let dir = d.app.cfg.user_home.join(".claude/projects").join(mangled);
    std::fs::create_dir_all(&dir).expect("transcript dir");
    let records = [
        json!({"type": "user", "uuid": "turn-1", "timestamp": "2026-10-01T10:00:00Z",
               "isMeta": true, "origin": {"kind": "peer"},
               "message": {"content":
                   "Another Claude session sent a message:\n[msg #7 from USER · chat] check the pricing page"}}),
        json!({"type": "assistant", "uuid": "a1", "timestamp": "2026-10-01T10:00:02Z",
               "message": {"content": [
                   {"type": "tool_use", "id": "nav", "name": "mcp__playwright__browser_navigate",
                    "input": {"url": "https://example.com/pricing"}},
                   {"type": "tool_use", "id": "click", "name": "mcp__playwright__browser_click",
                    "input": {"element": "Plans tab", "ref": "e12"}},
                   {"type": "tool_use", "id": "own", "name": "mcp__claude-in-chrome__navigate",
                    "input": {"url": "https://mail.example.com/"}},
                   {"type": "tool_use", "id": "other", "name": "Bash",
                    "input": {"command": "ls"}}]}}),
    ];
    let lines: Vec<String> = records.iter().map(Value::to_string).collect();
    std::fs::write(dir.join("session.jsonl"), lines.join("\n") + "\n").expect("transcript");

    let log = c
        .request(json!({"type": "list_browser_activity", "bot_id": id}))
        .await;
    assert_eq!(log["type"], "browser_activity", "{log}");
    let activity = log["activity"].as_array().expect("activity");
    assert_eq!(activity.len(), 3, "{log}");
    // Newest first, each with its turn and what started it.
    assert_eq!(activity[0]["browser"], "owners_chrome");
    assert_eq!(activity[0]["title"], "Opened mail.example.com");
    assert_eq!(activity[1]["title"], "Clicked Plans tab");
    assert_eq!(activity[2]["browser"], "own");
    assert_eq!(activity[2]["subtitle"], "https://example.com/pricing");
    assert_eq!(activity[2]["turn_id"], "turn-1");
    assert_eq!(activity[2]["trigger"]["kind"], "owner");
}
