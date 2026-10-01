//! Linked projects: a project on the Mac and one on the PC working as one
//! team. Both daemons run in this process, paired over a real peer link, so
//! linking, mirroring, unlinking and bots created across machines all run
//! for real. See "Linked projects" in `docs/peer-bots.md`.

mod common;

use bus::TaskState;
use common::peers::{bot_named, paired, project, wait_until, Paired};
use common::tasks::{drain_until, error_text};
use common::*;
use serde_json::{json, Value};

/// The Mac's "app" with `lead`, the PC's "app" with `windev`, linked.
struct Linked {
    p: Paired,
    mac_app: String,
    win_app: String,
    lead: McpClient,
}

async fn linked() -> Linked {
    let mut p = paired().await;
    let mac_app = project(&mut p.mac_client, "app").await;
    let lead = create_bot(&mut p.mac_client, &mac_app, "lead").await;
    let win_app = project(&mut p.win_client, "app").await;
    create_bot(&mut p.win_client, &win_app, "windev").await;
    let reply = link(&mut p, &mac_app, Some(&win_app)).await;
    assert_eq!(reply["type"], "project", "{reply}");
    let token = p
        .mac
        .app
        .secrets
        .bot_token(lead["id"].as_str().expect("id"))
        .expect("token");
    Linked {
        lead: McpClient::new(&p.mac, &token),
        p,
        mac_app,
        win_app,
    }
}

async fn link(p: &mut Paired, project_id: &str, remote: Option<&str>) -> Value {
    let mut req = json!({
        "type": "link_project", "project_id": project_id, "peer_id": p.mac_peer_id
    });
    if let Some(remote) = remote {
        req["remote_project_id"] = json!(remote);
    }
    p.mac_client.request(req).await
}

fn peer_of(bot: &bus::Bot) -> Option<&str> {
    bot.peer_id.as_deref()
}

#[tokio::test]
async fn linking_stands_every_bot_in_on_both_sides() {
    let mut t = linked().await;
    let (mac, win) = (&t.p.mac, &t.p.win);

    let windev = bot_named(mac, &t.mac_app, "windev").expect("windev stands in on the Mac");
    assert_eq!(peer_of(&windev), Some(t.p.mac_peer_id.as_str()));
    let lead = bot_named(win, &t.win_app, "lead").expect("lead stands in on the PC");
    assert_eq!(peer_of(&lead), Some(t.p.win_peer_id.as_str()));
    // Stand-ins run on their own machine and count there.
    assert_eq!(mac.app.db.count_live_bots(&t.mac_app).expect("count"), 1);

    let projects =
        t.p.mac_client
            .request(json!({"type": "list_projects"}))
            .await;
    let app = &projects["projects"][0];
    let links = &app["links"][0];
    assert_eq!(links["peer_id"], t.p.mac_peer_id.as_str(), "{app}");
    assert_eq!(links["peer_name"], "win");
    assert_eq!(links["online"], true);
    assert_eq!(links["remote_project_id"], t.win_app.as_str());
    assert_eq!(links["remote_project_name"], "app");
    assert!(links["linked_at"].is_string());

    // The PC sees the same link from its side, and the Mac's project list
    // through the peer names the project it is linked with.
    let theirs =
        t.p.win_client
            .request(json!({"type": "list_projects"}))
            .await;
    assert_eq!(
        theirs["projects"][0]["links"][0]["remote_project_id"],
        t.mac_app.as_str()
    );
    let listed =
        t.p.mac_client
            .request(json!({"type": "list_peer_projects", "peer_id": t.p.mac_peer_id}))
            .await;
    assert_eq!(listed["type"], "peer_projects", "{listed}");
    assert_eq!(listed["projects"][0]["id"], t.win_app.as_str());
    assert_eq!(listed["projects"][0]["bot_count"], 1);
    assert_eq!(
        listed["projects"][0]["linked_project_id"],
        t.mac_app.as_str()
    );

    // Bots address each other by name across machines.
    t.lead
        .call(
            "send_message",
            json!({"to": "windev", "kind": "note", "body": "hello from the mac"}),
        )
        .await;
}

#[tokio::test]
async fn linking_without_a_remote_project_creates_one_there() {
    let mut p = paired().await;
    let tools = project(&mut p.mac_client, "tools").await;
    create_bot(&mut p.mac_client, &tools, "builder").await;
    let reply = link(&mut p, &tools, None).await;
    assert_eq!(reply["type"], "project", "{reply}");
    let remote = reply["project"]["links"][0]["remote_project_id"]
        .as_str()
        .expect("remote id")
        .to_string();
    let created = p
        .win
        .app
        .db
        .get_project(&remote)
        .expect("db")
        .expect("project");
    assert_eq!(created.name, "tools");
    assert!(bot_named(&p.win, &remote, "builder").is_some());

    // Linking the same project through the same peer again is a conflict.
    let again = link(&mut p, &tools, None).await;
    assert_eq!(again["code"], "conflict", "{again}");
}

#[tokio::test]
async fn bots_created_renamed_and_archived_after_the_link_are_mirrored() {
    let mut t = linked().await;
    let designer = create_bot(&mut t.p.mac_client, &t.mac_app, "designer").await;
    let designer_id = designer["id"].as_str().expect("id").to_string();
    let (win, win_app) = (&t.p.win, t.win_app.clone());
    wait_until("the new bot stands in on the PC", || {
        bot_named(win, &win_app, "designer").is_some()
    })
    .await;

    let renamed =
        t.p.mac_client
            .request(json!({
                "type": "update_bot", "bot_id": designer_id, "name": "artist",
                "description": "draws things"
            }))
            .await;
    assert_eq!(renamed["type"], "bot", "{renamed}");
    wait_until("the rename reaches the PC", || {
        bot_named(win, &win_app, "artist").is_some_and(|b| b.description == "draws things")
    })
    .await;

    let deleted =
        t.p.mac_client
            .request(json!({"type": "delete_bot", "bot_id": designer_id}))
            .await;
    assert_eq!(deleted["type"], "ok", "{deleted}");
    wait_until("the archive reaches the PC", || {
        bot_named(win, &win_app, "artist").is_none()
    })
    .await;
}

#[tokio::test]
async fn a_link_that_would_clash_names_is_refused() {
    let mut p = paired().await;
    let mac_app = project(&mut p.mac_client, "app").await;
    create_bot(&mut p.mac_client, &mac_app, "lead").await;
    create_bot(&mut p.mac_client, &mac_app, "tester").await;
    let win_app = project(&mut p.win_client, "app").await;
    create_bot(&mut p.win_client, &win_app, "lead").await;

    let refused = link(&mut p, &mac_app, Some(&win_app)).await;
    assert_eq!(refused["type"], "error", "{refused}");
    assert_eq!(refused["code"], "conflict");
    assert!(refused["message"]
        .as_str()
        .expect("message")
        .contains("lead"));
    assert!(!refused["message"]
        .as_str()
        .expect("message")
        .contains("tester"));
    assert!(p
        .mac
        .app
        .db
        .project_links(&mac_app)
        .expect("links")
        .is_empty());
    assert!(p
        .win
        .app
        .db
        .project_links(&win_app)
        .expect("links")
        .is_empty());
}

#[tokio::test]
async fn unlinking_from_either_side_archives_the_stand_ins_on_both() {
    let mut t = linked().await;
    // An open task held by a stand-in is cancelled, as deleting a bot does.
    let sent = t
        .lead
        .call(
            "send_message",
            json!({"to": "windev", "kind": "task", "body": "port the updater"}),
        )
        .await;
    let task_id = sent["task_id"].as_str().expect("task").to_string();

    let unlinked =
        t.p.win_client
            .request(json!({
                "type": "unlink_project", "project_id": t.win_app, "peer_id": t.p.win_peer_id
            }))
            .await;
    assert_eq!(unlinked["type"], "project", "{unlinked}");
    assert_eq!(unlinked["project"]["links"], json!([]));
    assert!(bot_named(&t.p.win, &t.win_app, "lead").is_none());
    let (mac, mac_app) = (&t.p.mac, t.mac_app.clone());
    wait_until("the Mac drops its half", || {
        mac.app
            .db
            .project_links(&mac_app)
            .expect("links")
            .is_empty()
            && bot_named(mac, &mac_app, "windev").is_none()
    })
    .await;
    let task = mac.app.db.get_task(&task_id).expect("db").expect("task");
    assert_eq!(task.state, TaskState::Cancelled);
    drain_until(&mut t.lead, "Task cancelled").await;
    // History is kept: the stand-in is archived, not gone.
    let archived = mac
        .app
        .db
        .list_bots_with_archived(Some(&mac_app))
        .expect("bots");
    assert!(archived
        .iter()
        .any(|b| b.is_linked() && b.deleted_at.is_some()));
}

#[tokio::test]
async fn revoking_the_peer_unlinks_its_projects() {
    let mut t = linked().await;
    let revoked =
        t.p.mac_client
            .request(json!({"type": "revoke_peer", "peer_id": t.p.mac_peer_id}))
            .await;
    assert_eq!(revoked["type"], "peer", "{revoked}");
    let (mac, win) = (&t.p.mac, &t.p.win);
    let (mac_app, win_app) = (t.mac_app.clone(), t.win_app.clone());
    wait_until("both sides drop the link", || {
        mac.app
            .db
            .project_links(&mac_app)
            .expect("links")
            .is_empty()
            && win
                .app
                .db
                .project_links(&win_app)
                .expect("links")
                .is_empty()
            && bot_named(mac, &mac_app, "windev").is_none()
            && bot_named(win, &win_app, "lead").is_none()
    })
    .await;
}

#[tokio::test]
async fn create_bot_with_a_peer_creates_it_on_that_machine() {
    let mut t = linked().await;
    let created =
        t.p.mac_client
            .request(json!({
                "type": "create_bot", "project_id": t.mac_app, "peer_id": t.p.mac_peer_id,
                "name": "winqa", "description": "tests the windows build"
            }))
            .await;
    assert_eq!(created["type"], "bot", "{created}");
    assert_eq!(created["bot"]["peer"]["name"], "win");
    assert_eq!(created["bot"]["name"], "winqa");
    let real = bot_named(&t.p.win, &t.win_app, "winqa").expect("created on the PC");
    assert!(!real.is_linked());
    assert_eq!(real.runtime, t.p.win.app.cfg.default_bot_runtime);

    // Only through a link.
    let solo = project(&mut t.p.mac_client, "solo").await;
    let refused =
        t.p.mac_client
            .request(json!({
                "type": "create_bot", "project_id": solo, "peer_id": t.p.mac_peer_id,
                "name": "nowhere"
            }))
            .await;
    assert_eq!(refused["code"], "not_linked", "{refused}");
}

#[tokio::test]
async fn a_bot_creates_and_manages_a_bot_on_the_linked_machine() {
    let mut t = linked().await;
    let created = t
        .lead
        .call(
            "create_bot",
            json!({"name": "winhelper", "description": "helps on windows", "machine": "win"}),
        )
        .await;
    assert_eq!(created["machine"], "win", "{created}");
    let real = bot_named(&t.p.win, &t.win_app, "winhelper").expect("created on the PC");
    let stand_in = bot_named(&t.p.mac, &t.mac_app, "winhelper").expect("stands in here");
    let lead = bot_named(&t.p.mac, &t.mac_app, "lead").expect("lead");
    assert_eq!(
        stand_in.created_by_bot_id.as_deref(),
        Some(lead.id.as_str())
    );

    t.lead
        .call(
            "update_bot",
            json!({"name": "winhelper", "description": "signs installers"}),
        )
        .await;
    let updated = t.p.win.app.db.get_bot(&real.id).expect("db").expect("bot");
    assert_eq!(updated.description, "signs installers");

    t.lead
        .call("delete_bot", json!({"name": "winhelper", "reason": "done"}))
        .await;
    let gone = t.p.win.app.db.get_bot(&real.id).expect("db").expect("bot");
    assert!(gone.deleted_at.is_some());
    assert!(bot_named(&t.p.mac, &t.mac_app, "winhelper").is_none());

    // The PC's own bots are not the lead's to manage.
    let refused = t
        .lead
        .call_raw("delete_bot", json!({"name": "windev"}))
        .await;
    assert!(error_text(&refused).contains("not created by you"));
}

#[tokio::test]
async fn a_bot_cannot_create_on_a_machine_its_project_is_not_linked_with() {
    let mut p = paired().await;
    let solo = project(&mut p.mac_client, "solo").await;
    let bot = create_bot(&mut p.mac_client, &solo, "loner").await;
    let token = p
        .mac
        .app
        .secrets
        .bot_token(bot["id"].as_str().expect("id"))
        .expect("token");
    let mut loner = McpClient::new(&p.mac, &token);
    let refused = loner
        .call_raw("create_bot", json!({"name": "far", "machine": "win"}))
        .await;
    assert!(error_text(&refused).contains("not linked"), "{refused}");
    assert!(p.win.app.db.list_bots(None).expect("bots").is_empty());
}

#[tokio::test]
async fn the_handshake_names_the_daemon_and_the_capability() {
    let d = spawn_daemon().await;
    let hello = raw_hello(&d, d.app.secrets.client_token()).await;
    assert_eq!(
        hello["daemon_id"],
        d.app.db.daemon_id().expect("id").as_str()
    );
    let caps = hello["capabilities"].as_array().expect("caps");
    assert!(caps.contains(&json!("linked_projects")));
}
