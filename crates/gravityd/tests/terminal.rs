//! Terminal replay on the wire: what an attach sends, and in how many frames.

mod common;

use common::*;
use futures::StreamExt;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message as WsMsg;

/// Every `term` push for `bot_id` up to and including `through`, in order.
async fn collect_replay(c: &mut WsClient, bot_id: &str, through: u64) -> Vec<Value> {
    let mut frames = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let frame = tokio::time::timeout_at(deadline, c.rx.next())
            .await
            .expect("timed out collecting replay")
            .expect("stream ended")
            .expect("ws error");
        let WsMsg::Text(text) = frame else {
            continue;
        };
        let v: Value = serde_json::from_str(&text).expect("json frame");
        if v["type"] != "term" || v["bot_id"] != bot_id {
            continue;
        }
        let seq = v["seq"].as_u64().expect("seq");
        frames.push(v);
        if seq >= through {
            return frames;
        }
    }
}

/// A pty hands output over one small read at a time, and a full ring of them
/// used to replay as one push per read — thousands of frames for a client to
/// parse and paint one by one, which is what a busy machine could not keep up
/// with. The replay now travels merged, and so does live output that piles up
/// while a client is slow.
#[tokio::test]
async fn a_replay_arrives_merged_rather_than_one_push_per_write() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let bot = create_bot(&mut c, &project_id, "alice").await;
    let bot_id = bot["id"].as_str().expect("bid").to_string();
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;
    c.request(json!({"type": "attach", "bot_id": bot_id, "after_seq": 0}))
        .await;

    // The double echoes every input as its own output frame.
    for n in 0..300 {
        c.send(json!({"type": "input", "bot_id": bot_id, "data": format!("w{n:03} ")}))
            .await;
    }
    c.wait_for(|v| v["type"] == "term" && v["data"].as_str().is_some_and(|s| s.contains("w299")))
        .await;
    let ring = d.app.supervisor.term(&bot_id).expect("terminal");
    assert!(
        ring.latest_seq() > 300,
        "one frame per write reached the ring"
    );

    // A fresh client gets the whole ring back in a handful of pushes, the
    // last of which carries the cursor the reply promised.
    let mut fresh = WsClient::connect(&d).await;
    let attached = fresh
        .request(json!({"type": "attach", "bot_id": bot_id}))
        .await;
    assert_eq!(attached["type"], "attached", "{attached}");
    let through = attached["seq"].as_u64().expect("seq");
    let frames = collect_replay(&mut fresh, &bot_id, through).await;
    assert!(
        frames.len() <= 2,
        "{} pushes for {} frames of ring",
        frames.len(),
        through
    );
    let text: String = frames
        .iter()
        .map(|f| f["data"].as_str().expect("data"))
        .collect();
    assert!(
        text.contains("double runtime ready"),
        "greeting first: {text}"
    );
    assert!(
        text.contains("w000 ") && text.contains("w299 "),
        "every write kept: {text}"
    );
    assert_eq!(frames.last().expect("a frame")["seq"], json!(through));

    // Live output after the replay keeps flowing, contiguous with the cursor.
    fresh
        .send(json!({"type": "input", "bot_id": bot_id, "data": "after-replay"}))
        .await;
    let live = fresh
        .wait_for(|v| {
            v["type"] == "term"
                && v["data"]
                    .as_str()
                    .is_some_and(|s| s.contains("after-replay"))
        })
        .await;
    assert!(live["seq"].as_u64().expect("seq") > through);
}
