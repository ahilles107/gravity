//! A stand-in for Chrome's DevTools endpoint, so browser tests need no
//! browser.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as WsMsg;

use super::TestDaemon;

/// The browser profile directory of a bot on `d`.
pub fn profile_of(d: &TestDaemon, bot_id: &str) -> PathBuf {
    let bot = d.app.db.get_bot(bot_id).expect("db").expect("bot");
    PathBuf::from(&bot.workspace_path)
        .parent()
        .expect("root")
        .join("browser/profile")
}

/// Points a bot's profile at a fake DevTools endpoint, as Chrome does when it
/// starts.
pub fn start_browser(d: &TestDaemon, bot_id: &str, port: u16) {
    let profile = profile_of(d, bot_id);
    std::fs::create_dir_all(&profile).expect("mkdir");
    std::fs::write(
        profile.join("DevToolsActivePort"),
        format!("{port}\n/devtools/browser/x"),
    )
    .expect("port");
}

/// Serves `/json/list` with one page, and that page's WebSocket answering
/// `Page.startScreencast` with a frame, keeping connections open as Chrome
/// does. Returns the HTTP port.
pub async fn fake_devtools(title: &'static str, screencasts: Arc<AtomicUsize>) -> u16 {
    let ws = TcpListener::bind("127.0.0.1:0").await.expect("bind ws");
    let ws_port = ws.local_addr().expect("addr").port();
    tokio::spawn(async move {
        while let Ok((stream, _)) = ws.accept().await {
            screencasts.fetch_add(1, Ordering::SeqCst);
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
            // Like Chrome: the connection stays open whatever the request
            // asked, so the daemon has to stop at Content-Length.
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(30)).await;
                drop(stream);
            });
        }
    });
    port
}
