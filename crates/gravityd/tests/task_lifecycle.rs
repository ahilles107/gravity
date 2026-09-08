//! Ending a task: cancellation by the requester, deadline expiry, and the
//! artifacts a result carries.

mod common;

use std::time::Duration;

use bus::MAX_TASK_FANOUT;
use common::tasks::{error_text, peer_messages, project_with_bots};
use common::*;
use serde_json::{json, Value};

#[tokio::test]
async fn the_requester_can_cancel_a_task_its_delegate_orphaned() {
    let (p, mut clients) = project_with_bots(&["alice", "bob"]).await;

    let mut task_ids = Vec::new();
    for i in 0..MAX_TASK_FANOUT {
        let sent = clients[0]
            .call(
                "send_message",
                json!({"to": "bob", "body": format!("job {i}"), "kind": "task"}),
            )
            .await;
        task_ids.push(sent["task_id"].as_str().expect("task id").to_string());
    }

    // The assignee cannot cancel what was delegated to it — that is
    // complete_task's job.
    let refused = clients[1]
        .call_raw("cancel_task", json!({"task_id": task_ids[0]}))
        .await;
    assert!(
        error_text(&refused).contains("not yours to cancel"),
        "{refused}"
    );

    // The requester finds the ids it delegated without having kept them.
    let ledger = clients[0].call("check_inbox", json!({})).await;
    let delegated: Vec<&str> = ledger["delegated_tasks"]
        .as_array()
        .expect("delegated_tasks")
        .iter()
        .map(|t| t["task_id"].as_str().expect("task_id"))
        .collect();
    assert_eq!(delegated.len(), task_ids.len(), "{ledger}");
    for id in &task_ids {
        assert!(delegated.contains(&id.as_str()), "{ledger}");
    }
    assert_eq!(ledger["delegated_tasks"][0]["to"], "bob", "{ledger}");

    let cancelled = clients[0]
        .call(
            "cancel_task",
            json!({"task_id": task_ids[0], "reason": "bob went quiet"}),
        )
        .await;
    assert_eq!(cancelled["state"], "cancelled", "{cancelled}");
    assert_eq!(cancelled["notified"], true, "{cancelled}");
    assert_eq!(
        p.d.app
            .db
            .get_task(&task_ids[0])
            .expect("query")
            .expect("task")
            .state,
        bus::TaskState::Cancelled
    );

    // Cancelling twice is refused, and the freed slot takes a new delegation.
    let again = clients[0]
        .call_raw("cancel_task", json!({"task_id": task_ids[0]}))
        .await;
    assert!(error_text(&again).contains("already cancelled"), "{again}");
    clients[0]
        .call(
            "send_message",
            json!({"to": "bob", "body": "job again", "kind": "task"}),
        )
        .await;

    // The assignee is told to stop, with the reason. Delivery is asynchronous,
    // so poll rather than read the inbox once.
    let mut notice = None;
    for _ in 0..40 {
        let inbox = clients[1].call("check_inbox", json!({})).await;
        notice = inbox["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|m| m["body"].as_str().unwrap_or("").contains("was cancelled"))
            .cloned();
        if notice.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let notice = notice.expect("no cancellation note reached the assignee");
    assert_eq!(notice["kind"], "note", "{notice}");
    // The canceller's own name is on it: a bot's words must not arrive wearing
    // the daemon's identity, which the envelope renders as USER.
    assert_eq!(notice["from"], "alice", "{notice}");
    assert!(
        notice["body"]
            .as_str()
            .expect("body")
            .contains("bob went quiet"),
        "{notice}"
    );
}

#[tokio::test]
async fn a_finished_task_cannot_be_cancelled_out_from_under_its_result() {
    let (_p, mut clients) = project_with_bots(&["alice", "bob"]).await;

    let sent = clients[0]
        .call(
            "send_message",
            json!({"to": "bob", "body": "job", "kind": "task"}),
        )
        .await;
    let task_id = sent["task_id"].as_str().expect("task id").to_string();

    clients[1]
        .call("complete_task", json!({"task_id": task_id, "result": "ok"}))
        .await;

    let refused = clients[0]
        .call_raw(
            "cancel_task",
            json!({"task_id": task_id, "reason": "too late"}),
        )
        .await;
    assert!(error_text(&refused).contains("already done"), "{refused}");
}

#[tokio::test]
async fn a_chain_past_the_hop_limit_delivers_nothing() {
    let (_p, mut clients) = project_with_bots(&["a", "b", "c", "d", "e", "f"]).await;
    let names = ["a", "b", "c", "d", "e", "f"];

    // a → b → c → d → e uses the full depth of 4.
    for i in 0..4 {
        clients[i]
            .call(
                "send_message",
                json!({"to": names[i + 1], "body": "pass it on", "kind": "task"}),
            )
            .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        clients[i + 1].call("check_inbox", json!({})).await;
    }

    // The fifth hop is refused before anything is delivered.
    let refused = clients[4]
        .call_raw(
            "send_message",
            json!({"to": "f", "body": "pass it on", "kind": "task"}),
        )
        .await;
    assert!(error_text(&refused).contains("hops deep"), "{refused}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let inbox = clients[5].call("check_inbox", json!({})).await;
    assert!(
        peer_messages(&inbox).is_empty(),
        "refused send was delivered: {inbox}"
    );
}

#[tokio::test]
async fn an_overdue_task_expires_and_notifies_each_end_once() {
    let (p, mut clients) = project_with_bots(&["alice", "bob"]).await;
    let alice = &mut clients[0];

    // A delegated task carries the default deadline.
    let sent = alice
        .call(
            "send_message",
            json!({"to": "bob", "body": "long job", "kind": "task"}),
        )
        .await;
    let delegated =
        p.d.app
            .db
            .get_task(sent["task_id"].as_str().expect("task id"))
            .expect("query")
            .expect("task");
    assert!(delegated.deadline_at.is_some(), "no default deadline set");

    // An already-overdue task, planted directly so the test does not have to
    // wait out a real deadline. The daemon's scheduler (100ms tick here)
    // sweeps it.
    let overdue =
        p.d.app
            .db
            .create_task(
                sent["message_id"].as_str().expect("mid"),
                Some(&p.ids[0]),
                &p.ids[1],
                Some(chrono::Utc::now() - chrono::Duration::hours(1)),
                1,
                &p.ids[0],
            )
            .expect("task");
    tokio::time::sleep(Duration::from_millis(600)).await;

    let swept =
        p.d.app
            .db
            .get_task(&overdue.id)
            .expect("query")
            .expect("task");
    assert_eq!(swept.state, bus::TaskState::Expired, "task not swept");
    // The healthy delegation is untouched.
    let live =
        p.d.app
            .db
            .get_task(&delegated.id)
            .expect("query")
            .expect("task");
    assert_eq!(live.state, bus::TaskState::Open);

    // Exactly one expiry note per end, despite several ticks having passed.
    let [alice, bob] = &mut clients[..] else {
        unreachable!()
    };
    let expiry_notes = |inbox: &Value| {
        inbox["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .filter(|m| {
                m["from"] == json!("system") && m["body"].as_str().unwrap_or("").contains("expired")
            })
            .count()
    };
    let alice_inbox = alice.call("check_inbox", json!({})).await;
    assert_eq!(expiry_notes(&alice_inbox), 1, "{alice_inbox}");
    let bob_inbox = bob.call("check_inbox", json!({})).await;
    assert_eq!(expiry_notes(&bob_inbox), 1, "{bob_inbox}");
}

#[tokio::test]
async fn artifacts_dir_is_provisioned_and_carried_in_results() {
    let (p, mut clients) = project_with_bots(&["alice", "bob"]).await;

    // The shared directory exists, the session was granted access to it on its
    // command line, and the workspace settings pre-approve nothing — allow
    // rules in a folder's settings.json make Claude Code's trust dialog warn
    // about pre-approved permissions.
    let project = &p.d.app.db.list_projects().expect("projects")[0];
    let artifacts = gravityd::paths::artifacts_dir(&p.d.app.cfg, &project.dir_name);
    assert!(artifacts.is_dir(), "missing {}", artifacts.display());
    let spawned = terminal(&p.d, &p.ids[0]);
    assert!(
        spawned.contains("--allowedTools"),
        "the session was spawned without the artifacts grant: {spawned}"
    );
    for rule in gravityd::paths::artifacts_allow_rules(&artifacts) {
        assert!(
            spawned.contains(&rule),
            "the session was spawned without {rule}: {spawned}"
        );
    }
    assert!(
        spawned.contains(&format!("--add-dir {}", artifacts.display())),
        "the artifacts dir is outside the workspace and must be added: {spawned}"
    );
    let bot = p.d.app.db.get_bot(&p.ids[0]).expect("query").expect("bot");
    let settings: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(&bot.workspace_path).join(".claude/settings.json"),
        )
        .expect("settings"),
    )
    .expect("parse settings");
    assert_eq!(
        settings["permissions"]["allow"],
        serde_json::json!([]),
        "workspace settings must not pre-approve permissions"
    );

    // complete_task carries artifact paths into the done body.
    let [alice, bob] = &mut clients[..] else {
        unreachable!()
    };
    let sent = alice
        .call(
            "send_message",
            json!({"to": "bob", "body": "write the report", "kind": "task"}),
        )
        .await;
    let report = artifacts.join("report.md").display().to_string();
    bob.call(
        "complete_task",
        json!({"task_id": sent["task_id"], "result": "Report written.",
               "artifacts": [report]}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let inbox = alice.call("check_inbox", json!({})).await;
    let msgs = peer_messages(&inbox);
    assert_eq!(msgs.len(), 1, "{inbox}");
    assert_eq!(msgs[0]["kind"], "done");
    let body = msgs[0]["body"].as_str().expect("body");
    assert!(body.contains("artifacts:"), "{body}");
    assert!(body.contains("report.md"), "{body}");
}
