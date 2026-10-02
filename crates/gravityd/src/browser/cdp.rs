//! Just enough of the Chrome DevTools Protocol to show a bot's browser: find
//! it, list its tabs, and stream one tab's screen.

use std::path::Path;

use anyhow::Context;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::Message;

/// A page open in the bot's browser.
#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    pub id: String,
    pub title: String,
    pub url: String,
    pub ws_url: String,
}

/// The DevTools port of the browser running on `profile`, which Chrome
/// writes to `DevToolsActivePort` when it starts. The file outlives a browser
/// that crashed, so a port is only a candidate until it answers.
pub fn port(profile: &Path) -> Option<u16> {
    let text = std::fs::read_to_string(profile.join("DevToolsActivePort")).ok()?;
    text.lines().next()?.trim().parse().ok()
}

/// The browser's pages, most recently active first (Chrome's own order).
pub async fn tabs(port: u16) -> anyhow::Result<Vec<Tab>> {
    let body = get(port, "/json/list").await?;
    let list: Vec<Value> = serde_json::from_str(&body).context("tab list")?;
    Ok(list
        .iter()
        .filter(|t| t["type"] == "page")
        .filter_map(|t| {
            Some(Tab {
                id: t["id"].as_str()?.to_string(),
                title: t["title"].as_str().unwrap_or_default().to_string(),
                url: t["url"].as_str().unwrap_or_default().to_string(),
                ws_url: t["webSocketDebuggerUrl"].as_str()?.to_string(),
            })
        })
        .collect())
}

/// A plain HTTP/1.1 GET on loopback: all DevTools' discovery needs.
async fn get(port: u16, path: &str) -> anyhow::Result<String> {
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        tokio::net::TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .context("browser did not answer")??;
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;
    let mut raw = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        stream.read_to_end(&mut raw),
    )
    .await
    .context("browser did not answer")??;
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .context("malformed DevTools reply")?;
    anyhow::ensure!(head.contains(" 200 "), "DevTools refused {path}");
    Ok(body.to_string())
}

/// Streams a tab's screen: calls `frame` with each JPEG (base64) and the
/// page's size, until the tab closes or `frame` returns false.
pub async fn screencast(
    tab: &Tab,
    mut frame: impl FnMut(&str, u64, u64) -> bool,
) -> anyhow::Result<()> {
    let (socket, _) = tokio_tungstenite::connect_async(tab.ws_url.as_str())
        .await
        .context("could not attach to the tab")?;
    let (mut sink, mut stream) = socket.split();
    let start = json!({
        "id": 1, "method": "Page.startScreencast",
        "params": { "format": "jpeg", "quality": 70, "maxWidth": 1600, "maxHeight": 1200 }
    });
    sink.send(Message::Text(start.to_string())).await?;
    let mut next_id = 2;
    while let Some(message) = stream.next().await {
        let Message::Text(text) = message? else {
            continue;
        };
        let event: Value = serde_json::from_str(&text).unwrap_or_default();
        if event["method"] != "Page.screencastFrame" {
            continue;
        }
        let params = &event["params"];
        // Chrome sends the next frame only once this one is acknowledged,
        // which keeps a slow client from being flooded.
        let ack = json!({
            "id": next_id, "method": "Page.screencastFrameAck",
            "params": { "sessionId": params["sessionId"] }
        });
        next_id += 1;
        sink.send(Message::Text(ack.to_string())).await?;
        let metadata = &params["metadata"];
        let keep_going = frame(
            params["data"].as_str().unwrap_or_default(),
            metadata["deviceWidth"].as_f64().unwrap_or_default() as u64,
            metadata["deviceHeight"].as_f64().unwrap_or_default() as u64,
        );
        if !keep_going {
            break;
        }
    }
    Ok(())
}
