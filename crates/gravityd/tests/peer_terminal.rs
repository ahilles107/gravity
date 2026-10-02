//! A linked bot's terminal from the other machine: the Mac watches and types
//! into the Windows bot's terminal through the peer link, and several
//! viewers there share one feed.

mod common;

use common::peers::{team, wait_until};
use common::*;
use serde_json::json;

#[tokio::test]
async fn the_other_machines_bot_terminal_can_be_watched_and_typed_into() {
    let mut t = team().await;
    let windev = t.linked_windev.clone();

    let attached = t
        .mac_client
        .request(json!({"type": "attach", "bot_id": windev, "after_seq": 0}))
        .await;
    assert_eq!(attached["type"], "attached", "{attached}");
    t.mac_client
        .wait_for(|v| {
            v["type"] == "term"
                && v["bot_id"] == windev.as_str()
                && v["data"]
                    .as_str()
                    .is_some_and(|s| s.contains("double runtime ready for windev"))
        })
        .await;

    // Typing on the Mac reaches the PC's terminal, whose echo comes back.
    t.mac_client
        .send(json!({"type": "input", "bot_id": windev, "data": "typed on the mac"}))
        .await;
    t.mac_client
        .wait_for(|v| {
            v["type"] == "term"
                && v["data"]
                    .as_str()
                    .is_some_and(|s| s.contains("typed on the mac"))
        })
        .await;

    // A second viewer (the phone) replays the mirror; the PC still feeds once.
    let mut phone = WsClient::connect(&t.mac).await;
    phone
        .request(json!({"type": "attach", "bot_id": windev, "after_seq": 0}))
        .await;
    phone
        .wait_for(|v| {
            v["type"] == "term"
                && v["data"]
                    .as_str()
                    .is_some_and(|s| s.contains("typed on the mac"))
        })
        .await;
    assert_eq!(t.win.app.peers.terms.feeding(), 1);

    // Resizes reach the real terminal without error, and the feed stops once
    // nobody on the Mac watches.
    t.mac_client
        .send(json!({"type": "resize", "bot_id": windev, "cols": 100, "rows": 30}))
        .await;
    t.mac_client
        .request(json!({"type": "detach", "bot_id": windev}))
        .await;
    assert_eq!(t.win.app.peers.terms.feeding(), 1);
    phone
        .request(json!({"type": "detach", "bot_id": windev}))
        .await;
    let win = &t.win;
    wait_until("the PC stops feeding", || {
        win.app.peers.terms.feeding() == 0
    })
    .await;

    // Watching again picks up where the mirror left off.
    let again = t
        .mac_client
        .request(json!({"type": "attach", "bot_id": windev, "after_seq": attached["seq"]}))
        .await;
    assert_eq!(again["type"], "attached", "{again}");
    wait_until("the PC feeds again", || win.app.peers.terms.feeding() == 1).await;
}
