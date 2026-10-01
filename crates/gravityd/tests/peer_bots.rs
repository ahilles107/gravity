//! Peer bots: two daemons paired over a peer link, a lead on one delegating
//! to a bot on the other. Both daemons run in this process on the runtime
//! double, so the whole path — pairing, linking, forwarding, task mirroring,
//! artifact transfer — runs for real without a second machine.

mod common;

use std::time::Duration;

use bus::TaskState;
use common::tasks::{drain_until, error_text};
use common::*;
use serde_json::{json, Value};

struct Team {
    mac: TestDaemon,
    win: TestDaemon,
    mac_client: WsClient,
    /// The Windows daemon's id for the peer row standing for the Mac.
    win_peer_id: String,
    /// The Mac's linked bot standing in for `windev`.
    linked_windev: String,
    lead: McpClient,
    windev: McpClient,
    windev_id: String,
}

async fn wait_until(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !check() {
        assert!(tokio::time::Instant::now() < deadline, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn project(c: &mut WsClient, name: &str) -> String {
    let created = c
        .request(json!({"type": "create_project", "name": name}))
        .await;
    created["project"]["id"].as_str().expect("pid").to_string()
}

/// A lead on "the Mac" and a Windows developer on "the PC", paired, with the
/// developer linked into the lead's project.
async fn team() -> Team {
    let mac = spawn_daemon().await;
    let win = spawn_daemon().await;
    let mut mac_client = WsClient::connect(&mac).await;
    let mut win_client = WsClient::connect(&win).await;

    let mac_project = project(&mut mac_client, "app").await;
    let lead = create_bot(&mut mac_client, &mac_project, "lead").await;
    let win_project = project(&mut win_client, "app").await;
    let windev = create_bot(&mut win_client, &win_project, "windev").await;
    let windev_id = windev["id"].as_str().expect("id").to_string();

    let invite = win_client
        .request(json!({
            "type": "create_peer_invite", "name": "mac",
            "url": format!("ws://{}/peer", win.addr)
        }))
        .await;
    assert_eq!(invite["type"], "peer", "{invite}");
    let win_peer_id = invite["peer"]["id"].as_str().expect("peer id").to_string();
    let added = mac_client
        .request(json!({
            "type": "add_peer", "name": "win", "invite": invite["invite"]
        }))
        .await;
    assert_eq!(added["type"], "peer", "{added}");
    let mac_peer_id = added["peer"]["id"].as_str().expect("peer id").to_string();
    wait_until("the link comes up", || {
        mac.app.peers.is_online(&mac_peer_id)
    })
    .await;

    let bots = mac_client
        .request(json!({"type": "list_peer_bots", "peer_id": mac_peer_id}))
        .await;
    assert_eq!(bots["bots"][0]["name"], "windev", "{bots}");
    let linked = mac_client
        .request(json!({
            "type": "link_peer_bot", "peer_id": mac_peer_id,
            "remote_bot_id": windev_id, "project_id": mac_project
        }))
        .await;
    assert_eq!(linked["type"], "bot", "{linked}");
    assert_eq!(linked["bot"]["peer"]["name"], "win");

    let lead_token = mac
        .app
        .secrets
        .bot_token(lead["id"].as_str().expect("id"))
        .expect("token");
    let windev_token = win.app.secrets.bot_token(&windev_id).expect("token");
    Team {
        lead: McpClient::new(&mac, &lead_token),
        windev: McpClient::new(&win, &windev_token),
        linked_windev: linked["bot"]["id"].as_str().expect("id").to_string(),
        mac,
        win,
        mac_client,
        win_peer_id,
        windev_id,
    }
}

fn find<'a>(messages: &'a [Value], needle: &str) -> &'a Value {
    messages
        .iter()
        .find(|m| m["body"].as_str().is_some_and(|b| b.contains(needle)))
        .unwrap_or_else(|| panic!("no message containing {needle:?} in {messages:?}"))
}

#[tokio::test]
async fn a_task_crosses_to_the_peer_and_its_result_and_files_come_back() {
    let mut t = team().await;

    let bots = t.lead.call("list_bots", json!({})).await;
    let listed = bots["bots"]
        .as_array()
        .expect("bots")
        .iter()
        .find(|b| b["name"] == "windev")
        .expect("windev listed");
    assert_eq!(listed["machine"], "win");
    assert_eq!(listed["online"], true);

    let sent = t
        .lead
        .call(
            "send_message",
            json!({"to": "windev", "kind": "task", "body": "build the windows installer"}),
        )
        .await;
    let mac_task = sent["task_id"].as_str().expect("task id").to_string();

    // On the PC the task arrives from a linked "lead", with a task of its own.
    let inbox = drain_until(&mut t.windev, "build the windows installer").await;
    let task_msg = find(&inbox, "build the windows installer");
    assert_eq!(task_msg["from"], "lead");
    let win_task = task_msg["task_id"]
        .as_str()
        .expect("mirrored task")
        .to_string();

    let windev = t
        .win
        .app
        .db
        .get_bot(&t.windev_id)
        .expect("db")
        .expect("bot");
    let out = std::path::Path::new(&windev.workspace_path).join("out");
    std::fs::create_dir_all(&out).expect("mkdir");
    std::fs::write(out.join("setup.txt"), "installer bytes").expect("write");
    t.windev
        .call(
            "complete_task",
            json!({
                "task_id": win_task, "result": "installer built",
                "artifacts": ["out/setup.txt", "../../../../secrets/client.token"]
            }),
        )
        .await;

    // Back on the Mac: the lead's task is closed by the peer's result, and
    // the file it listed is on this disk.
    let inbox = drain_until(&mut t.lead, "installer built").await;
    let done = find(&inbox, "installer built");
    assert_eq!(done["kind"], "done");
    let body = done["body"].as_str().expect("body");
    let local = body
        .lines()
        .find_map(|l| l.strip_prefix("- ").filter(|p| p.ends_with("setup.txt")))
        .unwrap_or_else(|| panic!("no transferred file in {body}"));
    assert_eq!(
        std::fs::read_to_string(local).expect("artifact"),
        "installer bytes"
    );
    // A path outside the bot's own files is listed, never sent.
    assert!(body.contains("not transferred"), "{body}");
    let task = t.mac.app.db.get_task(&mac_task).expect("db").expect("task");
    assert_eq!(task.state, TaskState::Done);

    // The lead's session saw the sender's machine in the envelope.
    let lead_id = t
        .mac
        .app
        .db
        .get_task(&mac_task)
        .unwrap()
        .unwrap()
        .from_bot_id
        .unwrap();
    wait_until("the result reaches the lead's session", || {
        terminal(&t.mac, &lead_id).contains("from WINDEV @ WIN · done")
    })
    .await;
}

#[tokio::test]
async fn cancelling_on_one_side_closes_the_task_on_the_other() {
    let mut t = team().await;
    let sent = t
        .lead
        .call(
            "send_message",
            json!({"to": "windev", "kind": "task", "body": "port the updater"}),
        )
        .await;
    let inbox = drain_until(&mut t.windev, "port the updater").await;
    let win_task = find(&inbox, "port the updater")["task_id"]
        .as_str()
        .expect("task id")
        .to_string();

    t.lead
        .call(
            "cancel_task",
            json!({"task_id": sent["task_id"], "reason": "not needed"}),
        )
        .await;
    drain_until(&mut t.windev, "was cancelled").await;
    let task = t.win.app.db.get_task(&win_task).expect("db").expect("task");
    assert_eq!(task.state, TaskState::Cancelled);
    let refused = t
        .windev
        .call_raw("complete_task", json!({"task_id": win_task, "result": "x"}))
        .await;
    assert!(error_text(&refused).contains("already cancelled"));
}

#[tokio::test]
async fn the_owner_can_message_a_linked_bot_from_the_other_machine() {
    let mut t = team().await;
    let sent = t
        .mac_client
        .request(json!({
            "type": "send_user_message", "to_bot_id": t.linked_windev,
            "body": "which SDK are you on?"
        }))
        .await;
    assert_eq!(sent["type"], "message", "{sent}");
    let inbox = drain_until(&mut t.windev, "which SDK").await;
    let chat = find(&inbox, "which SDK");
    assert_eq!(chat["from"], "user");
    assert_eq!(chat["kind"], "chat");
}

#[tokio::test]
async fn a_revoked_peer_is_cut_off() {
    let mut t = team().await;
    let mut win_client = WsClient::connect(&t.win).await;
    let revoked = win_client
        .request(json!({"type": "revoke_peer", "peer_id": t.win_peer_id}))
        .await;
    assert_eq!(revoked["type"], "peer", "{revoked}");
    let mac = &t.mac;
    wait_until("the Mac sees the link drop", || {
        mac.app
            .db
            .list_peers()
            .expect("peers")
            .iter()
            .all(|p| !mac.app.peers.is_online(&p.id))
    })
    .await;

    // Messages wait rather than fail while the peer is unreachable.
    t.lead
        .call(
            "send_message",
            json!({"to": "windev", "kind": "note", "body": "ping"}),
        )
        .await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let queued = t
        .mac
        .app
        .db
        .list_deliveries(Some(&t.linked_windev), None)
        .expect("deliveries");
    assert!(
        queued
            .iter()
            .all(|d| d.state != bus::DeliveryState::Delivered),
        "{queued:?}"
    );
}

#[tokio::test]
async fn the_peer_bot_can_ask_back_on_an_open_task() {
    let mut t = team().await;
    t.lead
        .call(
            "send_message",
            json!({"to": "windev", "kind": "task", "body": "sign the binaries"}),
        )
        .await;
    drain_until(&mut t.windev, "sign the binaries").await;
    t.windev
        .call(
            "send_message",
            json!({"to": "lead", "kind": "reply", "body": "which certificate?"}),
        )
        .await;
    let inbox = drain_until(&mut t.lead, "which certificate?").await;
    let question = find(&inbox, "which certificate?");
    assert_eq!(question["kind"], "reply");
    assert_eq!(question["from"], "windev");

    // Handing the work back up the chain is still a loop across machines.
    let refused = t
        .windev
        .call_raw(
            "send_message",
            json!({"to": "lead", "kind": "task", "body": "you do it"}),
        )
        .await;
    assert!(error_text(&refused).contains("loop"));
}
