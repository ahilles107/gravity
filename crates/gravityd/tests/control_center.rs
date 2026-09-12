//! The decision registry over the control plane: the owner's half.

mod common;

use common::tasks::{drain_until, project_with_bots};
use common::{raw_hello, WsClient};
use futures::StreamExt;
use serde_json::{json, Value};

/// Raise a decision as a bot and return its id.
async fn raised(clients: &mut [common::McpClient], who: usize, title: &str) -> String {
    let raised = clients[who]
        .call(
            "raise_decision",
            json!({"title": title, "body": "Spend is $40/day.", "tags": ["spend"]}),
        )
        .await;
    raised["decision"]["id"]
        .as_str()
        .expect("decision id")
        .to_string()
}

#[tokio::test]
async fn the_owner_answers_then_publishes_and_the_bot_hears_it() {
    let (pair, mut bots) = project_with_bots(&["auction"]).await;
    let id = raised(&mut bots, 0, "Pause the campaign?").await;
    let mut owner = WsClient::connect(&pair.d).await;

    let pending = owner.request(json!({"type": "list_decisions"})).await;
    assert_eq!(
        pending["decisions"].as_array().unwrap().len(),
        1,
        "{pending}"
    );
    assert_eq!(
        pending["decisions"][0]["raised_by"]["name"],
        json!("auction")
    );
    assert_eq!(pending["decisions"][0]["tags"], json!(["spend"]));

    // A draft is the owner's alone until it is published.
    let answered = owner
        .request(json!({
            "type": "answer_decision", "decision_id": id,
            "ruling_text": "Let it fire.", "ruling_reason": "We learn more from the pause."
        }))
        .await;
    assert_eq!(
        answered["decision"]["state"],
        json!("answered"),
        "{answered}"
    );
    assert!(drain_until(&mut bots[0], "· settled").await.is_empty());

    let published = owner
        .request(json!({
            "type": "publish_decisions",
            "items": [{"decision_id": id}]
        }))
        .await;
    assert_eq!(published["type"], json!("publish_result"), "{published}");
    assert_eq!(published["results"][0]["notified"], json!(["auction"]));

    let delivered = drain_until(&mut bots[0], "Let it fire.").await;
    assert!(!delivered.is_empty(), "the ruling never arrived");
}

#[tokio::test]
async fn the_badge_counts_what_the_owner_still_owes() {
    let (pair, mut bots) = project_with_bots(&["auction"]).await;
    let first = raised(&mut bots, 0, "Pause the campaign?").await;
    raised(&mut bots, 0, "Raise the daily cap?").await;
    let mut owner = WsClient::connect(&pair.d).await;

    let counts = owner
        .request(json!({"type": "count_pending_decisions"}))
        .await;
    assert_eq!(counts["counts"]["total"], json!(2), "{counts}");

    // Holding is the owner saying "later" out loud; it stops counting.
    owner
        .request(json!({
            "type": "hold_decision", "decision_id": first,
            "comment": "Tell me more about the Backblaze caps first."
        }))
        .await;
    let counts = owner
        .request(json!({"type": "count_pending_decisions"}))
        .await;
    assert_eq!(counts["counts"]["total"], json!(1), "{counts}");

    // And the asker is told, rather than left waiting on an answer today.
    let held = drain_until(&mut bots[0], "· held").await;
    assert!(
        held.iter().any(|m| m["body"]
            .as_str()
            .is_some_and(|b| b.contains("Backblaze caps") && b.contains("Parked, not refused"))),
        "{held:?}"
    );

    owner
        .request(json!({"type": "resume_decision", "decision_id": first}))
        .await;
    let counts = owner
        .request(json!({"type": "count_pending_decisions"}))
        .await;
    assert_eq!(counts["counts"]["total"], json!(2), "{counts}");
}

#[tokio::test]
async fn a_refusal_reaches_the_client_as_something_it_can_act_on() {
    let (pair, mut bots) = project_with_bots(&["auction"]).await;
    let id = raised(&mut bots, 0, "Pause the campaign?").await;
    let mut owner = WsClient::connect(&pair.d).await;

    // Publishing without a ruling would deliver a decision with no words.
    let refused = owner
        .request(json!({"type": "publish_decisions", "items": [{"decision_id": id}]}))
        .await;
    assert_eq!(refused["type"], json!("error"), "{refused}");
    assert_eq!(refused["code"], json!("conflict"), "{refused}");

    let missing = owner
        .request(json!({"type": "get_decision", "decision_id": "nope"}))
        .await;
    assert_eq!(missing["code"], json!("not_found"), "{missing}");

    let invalid = owner
        .request(json!({
            "type": "answer_decision", "decision_id": id, "ruling_text": "  "
        }))
        .await;
    assert_eq!(invalid["code"], json!("invalid_request"), "{invalid}");
}

#[tokio::test]
async fn a_read_only_device_can_look_but_not_rule() {
    let (pair, mut bots) = project_with_bots(&["auction"]).await;
    let id = raised(&mut bots, 0, "Pause the campaign?").await;
    let mut owner = WsClient::connect(&pair.d).await;
    let created = owner
        .request(json!({"type": "create_device", "name": "phone", "capabilities": ["read"]}))
        .await;
    let token = created["token"].as_str().expect("token").to_string();

    let hello = raw_hello(&pair.d, &token).await;
    assert_eq!(hello["type"], "hello_ok", "{hello}");
    assert!(
        hello["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("decisions")),
        "{hello}"
    );

    let mut phone = device_client(&pair.d, &token).await;
    let listed = phone.request(json!({"type": "list_decisions"})).await;
    assert_eq!(listed["type"], json!("decisions"), "{listed}");

    let refused = phone
        .request(json!({
            "type": "answer_decision", "decision_id": id, "ruling_text": "go"
        }))
        .await;
    assert_eq!(refused["code"], json!("forbidden"), "{refused}");
}

/// A connected client on a device token, past the handshake.
async fn device_client(d: &common::TestDaemon, token: &str) -> WsClient {
    let url = format!("ws://{}/ws", d.addr);
    let (socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("connect");
    let (tx, rx) = socket.split();
    let mut c = WsClient {
        tx,
        rx,
        next_req: 1,
    };
    c.request(json!({
        "type": "hello", "protocol_version": 2, "token": token, "client": "test/0"
    }))
    .await;
    c
}

#[tokio::test]
async fn running_the_fleet_is_not_the_same_grant_as_ruling_for_the_owner() {
    // `control` starts bots and sends messages. Answering *as the owner* is
    // what `approve` is for — a published ruling from a laptop credential
    // would otherwise be indistinguishable from one they typed.
    let (pair, mut bots) = project_with_bots(&["auction"]).await;
    let id = raised(&mut bots, 0, "Pause the campaign?").await;
    let mut owner = WsClient::connect(&pair.d).await;

    let created = owner
        .request(json!({
            "type": "create_device", "name": "laptop", "capabilities": ["read", "control"]
        }))
        .await;
    let mut laptop = device_client(&pair.d, created["token"].as_str().expect("token")).await;

    // It can still do everything `control` has always covered.
    let sent = laptop
        .request(json!({"type": "list_bots", "project_id": Value::Null}))
        .await;
    assert_eq!(sent["type"], json!("bots"), "{sent}");
    let commented = laptop
        .request(json!({
            "type": "comment_decision", "decision_id": id, "body": "the spend is confirmed"
        }))
        .await;
    assert_eq!(commented["type"], json!("decision_comment"), "{commented}");

    for ruling in [
        json!({"type": "answer_decision", "decision_id": id, "ruling_text": "go"}),
        json!({"type": "publish_decisions", "items": [{"decision_id": id}]}),
        json!({"type": "delete_decision", "decision_id": id}),
    ] {
        let refused = laptop.request(ruling.clone()).await;
        assert_eq!(refused["code"], json!("forbidden"), "{ruling}: {refused}");
        assert!(
            refused["message"].as_str().unwrap().contains("approve"),
            "the refusal should name the grant it wants: {refused}"
        );
    }

    // And a device the owner did grant `approve` rules normally.
    let created = owner
        .request(json!({
            "type": "create_device", "name": "phone", "capabilities": ["read", "approve"]
        }))
        .await;
    let mut phone = device_client(&pair.d, created["token"].as_str().expect("token")).await;
    let answered = phone
        .request(json!({
            "type": "answer_decision", "decision_id": id, "ruling_text": "go"
        }))
        .await;
    assert_eq!(answered["type"], json!("decision"), "{answered}");
}

#[tokio::test]
async fn an_update_reaches_a_second_client_without_it_asking() {
    let (pair, mut bots) = project_with_bots(&["auction"]).await;
    let mut watcher = WsClient::connect(&pair.d).await;
    let mut owner = WsClient::connect(&pair.d).await;
    let id = raised(&mut bots, 0, "Pause the campaign?").await;

    owner
        .request(json!({
            "type": "answer_decision", "decision_id": id, "ruling_text": "Let it fire."
        }))
        .await;
    let push = watcher
        .wait_for(|v: &Value| {
            v["type"] == json!("decision_update") && v["decision"]["state"] == json!("answered")
        })
        .await;
    assert_eq!(push["decision"]["id"], json!(id), "{push}");
    // The push carries the assembled view, not a bare row.
    assert_eq!(push["decision"]["raised_by"]["name"], json!("auction"));
    assert_eq!(push["decision"]["tags"], json!(["spend"]));
}

#[tokio::test]
async fn the_owner_edits_and_deletes_where_a_bot_cannot() {
    let (pair, mut bots) = project_with_bots(&["auction"]).await;
    let id = raised(&mut bots, 0, "Pause the campaign?").await;
    let mut owner = WsClient::connect(&pair.d).await;

    let edited = owner
        .request(json!({
            "type": "update_decision", "decision_id": id,
            "title": "Pause the Apple Ads campaign?", "priority": "urgent"
        }))
        .await;
    assert_eq!(
        edited["decision"]["title"],
        json!("Pause the Apple Ads campaign?")
    );
    assert_eq!(edited["decision"]["priority"], json!("urgent"));
    assert!(edited["decision"]["edited_at"].is_string(), "{edited}");

    let deleted = owner
        .request(json!({"type": "delete_decision", "decision_id": id}))
        .await;
    assert_eq!(deleted["type"], json!("ok"), "{deleted}");
    let gone = owner
        .request(json!({"type": "get_decision", "decision_id": id}))
        .await;
    assert_eq!(gone["code"], json!("not_found"), "{gone}");
}

#[tokio::test]
async fn a_settled_topic_returns_as_a_new_decision_on_current_facts() {
    let (pair, mut bots) = project_with_bots(&["patch"]).await;
    let id = raised(&mut bots, 0, "Pin Dozzle?").await;
    let mut owner = WsClient::connect(&pair.d).await;
    owner
        .request(json!({
            "type": "answer_decision", "decision_id": id, "ruling_text": "It stays unpinned."
        }))
        .await;
    owner
        .request(json!({"type": "publish_decisions", "items": [{"decision_id": id}]}))
        .await;

    let reopened = owner
        .request(json!({
            "type": "reopen_decision", "decision_id": id,
            "body": "1.5 changed the update cadence."
        }))
        .await;
    let new_id = reopened["decision"]["id"].as_str().unwrap();
    assert_ne!(new_id, id, "a reopen is a new record");
    assert_eq!(reopened["decision"]["supersedes_id"], json!(id));
    assert_eq!(reopened["decision"]["state"], json!("open"));
    // Tags come with it, so the topic stays findable under the same category.
    assert_eq!(reopened["decision"]["tags"], json!(["spend"]));

    let old = owner
        .request(json!({"type": "get_decision", "decision_id": id}))
        .await;
    assert_eq!(
        old["decision"]["state"],
        json!("settled"),
        "the old ruling stands"
    );
    assert_eq!(old["decision"]["superseded_by_id"], json!(new_id));
}

#[tokio::test]
async fn naming_a_lead_bot_is_checked_against_the_project() {
    let (pair, _bots) = project_with_bots(&["chief"]).await;
    let mut owner = WsClient::connect(&pair.d).await;
    let projects = owner.request(json!({"type": "list_projects"})).await;
    let project_id = projects["projects"][0]["id"].as_str().unwrap().to_string();

    let set = owner
        .request(json!({
            "type": "set_project_lead", "project_id": project_id, "bot_id": pair.ids[0]
        }))
        .await;
    assert_eq!(set["project"]["lead_bot_id"], json!(pair.ids[0]), "{set}");

    let cleared = owner
        .request(json!({"type": "set_project_lead", "project_id": project_id, "bot_id": null}))
        .await;
    assert!(cleared["project"]["lead_bot_id"].is_null(), "{cleared}");
}

#[tokio::test]
async fn a_relayed_ruling_is_marked_until_the_owner_confirms_it() {
    let (pair, mut bots) = project_with_bots(&["patch"]).await;
    let recorded = bots[0]
        .call(
            "record_decision",
            json!({
                "title": "Bump Forgejo to 16",
                "body": "Asked at my terminal.",
                "ruling_text": "let's bump forgejo to 16"
            }),
        )
        .await;
    let id = recorded["decision"]["id"].as_str().unwrap().to_string();
    let mut owner = WsClient::connect(&pair.d).await;

    let before = owner
        .request(json!({"type": "get_decision", "decision_id": id}))
        .await;
    let answered_by = before["decision"]["ruling"]["answered_by"]
        .as_str()
        .unwrap();
    assert!(answered_by.starts_with("owner-via-bot:"), "{answered_by}");

    let confirmed = owner
        .request(json!({"type": "confirm_decision", "decision_id": id}))
        .await;
    assert_eq!(
        confirmed["decision"]["ruling"]["answered_by"],
        json!("owner")
    );

    let again = owner
        .request(json!({"type": "confirm_decision", "decision_id": id}))
        .await;
    assert_eq!(again["code"], json!("conflict"), "{again}");
}
