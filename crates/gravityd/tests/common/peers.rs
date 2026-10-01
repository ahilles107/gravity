//! Two daemons paired over a peer link: a lead on "the Mac" and a Windows
//! developer on "the PC", linked into the lead's project.

use std::time::Duration;

use serde_json::{json, Value};

use super::{create_bot, spawn_daemon_with, McpClient, TestDaemon, WsClient};

pub struct Team {
    pub mac: TestDaemon,
    pub win: TestDaemon,
    pub mac_client: WsClient,
    /// The Windows daemon's id for the peer row standing for the Mac.
    pub win_peer_id: String,
    /// The Mac's linked bot standing in for `windev`.
    pub linked_windev: String,
    pub lead: McpClient,
    pub windev: McpClient,
    pub windev_id: String,
}

pub async fn wait_until(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !check() {
        assert!(tokio::time::Instant::now() < deadline, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

pub async fn project(c: &mut WsClient, name: &str) -> String {
    let created = c
        .request(json!({"type": "create_project", "name": name}))
        .await;
    created["project"]["id"].as_str().expect("pid").to_string()
}

/// A lead on "the Mac" and a Windows developer on "the PC", paired, with the
/// developer linked into the lead's project.
pub async fn team() -> Team {
    let mac = spawn_daemon_with(|cfg| cfg.user_home = cfg.home.join("user")).await;
    let win = spawn_daemon_with(|cfg| cfg.user_home = cfg.home.join("user")).await;
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

/// Two daemons paired over a peer link, with nothing linked yet.
pub struct Paired {
    pub mac: TestDaemon,
    pub win: TestDaemon,
    pub mac_client: WsClient,
    pub win_client: WsClient,
    /// The Mac's id for the peer row standing for the PC.
    pub mac_peer_id: String,
    /// The PC's id for the peer row standing for the Mac.
    pub win_peer_id: String,
}

pub async fn paired() -> Paired {
    let mac = spawn_daemon_with(|cfg| cfg.user_home = cfg.home.join("user")).await;
    let win = spawn_daemon_with(|cfg| cfg.user_home = cfg.home.join("user")).await;
    let mut mac_client = WsClient::connect(&mac).await;
    let mut win_client = WsClient::connect(&win).await;
    let (win_peer_id, mac_peer_id) = pair(&mac, &win, &mut mac_client, &mut win_client).await;
    Paired {
        mac,
        win,
        mac_client,
        win_client,
        mac_peer_id,
        win_peer_id,
    }
}

/// Pairs the two: the PC invites, the Mac dials. Returns the PC's and the
/// Mac's peer ids once the link is up.
pub async fn pair(
    mac: &TestDaemon,
    win: &TestDaemon,
    mac_client: &mut WsClient,
    win_client: &mut WsClient,
) -> (String, String) {
    let invite = win_client
        .request(json!({
            "type": "create_peer_invite", "name": "mac",
            "url": format!("ws://{}/peer", win.addr)
        }))
        .await;
    assert_eq!(invite["type"], "peer", "{invite}");
    let added = mac_client
        .request(json!({"type": "add_peer", "name": "win", "invite": invite["invite"]}))
        .await;
    assert_eq!(added["type"], "peer", "{added}");
    let win_peer_id = invite["peer"]["id"].as_str().expect("id").to_string();
    let mac_peer_id = added["peer"]["id"].as_str().expect("id").to_string();
    wait_until("the link comes up", || {
        mac.app.peers.is_online(&mac_peer_id) && win.app.peers.is_online(&win_peer_id)
    })
    .await;
    (win_peer_id, mac_peer_id)
}

/// The live bot named `name` in a project, if there is one.
pub fn bot_named(d: &TestDaemon, project_id: &str, name: &str) -> Option<bus::Bot> {
    d.app.db.get_bot_by_name(project_id, name).expect("db")
}

pub fn find<'a>(messages: &'a [Value], needle: &str) -> &'a Value {
    messages
        .iter()
        .find(|m| m["body"].as_str().is_some_and(|b| b.contains(needle)))
        .unwrap_or_else(|| panic!("no message containing {needle:?} in {messages:?}"))
}
