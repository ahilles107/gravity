//! Bus guardrails: reply budgets, terminal kinds and fan-out caps.

mod common;

use std::time::Duration;

use bus::{MAX_TASK_FANOUT, MAX_TASK_REPLIES};
use common::tasks::{error_text, peer_messages, project_with_bots};
use serde_json::json;

#[tokio::test]
async fn reply_budget_caps_ping_pong_but_never_the_result() {
    let (p, mut clients) = project_with_bots(&["alice", "bob"]).await;
    let [alice, bob] = &mut clients[..] else {
        unreachable!()
    };

    let sent = alice
        .call(
            "send_message",
            json!({"to": "bob", "body": "review PR #1", "kind": "task"}),
        )
        .await;
    let task_id = sent["task_id"].as_str().expect("task id").to_string();

    // Alternate replies in both directions; the budget is shared.
    for i in 0..MAX_TASK_REPLIES {
        let (from, to) = if i % 2 == 0 {
            (&mut *bob, "alice")
        } else {
            (&mut *alice, "bob")
        };
        let reply = from
            .call(
                "send_message",
                json!({"to": to, "body": format!("ping {i}"), "kind": "reply"}),
            )
            .await;
        assert!(reply["task_id"].is_null(), "reply opened a task: {reply}");
    }
    // Requester-direction replies must not have opened spurious tasks.
    assert!(
        p.d.app
            .db
            .open_tasks_for(&p.ids[0])
            .expect("tasks")
            .is_empty(),
        "a reply opened a task assigned to the requester"
    );

    let over = bob
        .call_raw(
            "send_message",
            json!({"to": "alice", "body": "one more", "kind": "reply"}),
        )
        .await;
    assert!(error_text(&over).contains("reply limit reached"), "{over}");
    let over = alice
        .call_raw(
            "send_message",
            json!({"to": "bob", "body": "one more", "kind": "reply"}),
        )
        .await;
    assert!(error_text(&over).contains("reply limit reached"), "{over}");

    // The cap never traps the result.
    bob.call(
        "complete_task",
        json!({"task_id": task_id, "result": "looks good"}),
    )
    .await;
}

#[tokio::test]
async fn replies_need_an_open_task_and_results_are_terminal() {
    let (_p, mut clients) = project_with_bots(&["alice", "bob"]).await;
    let [alice, bob] = &mut clients[..] else {
        unreachable!()
    };

    // No task between the pair yet: nothing to reply on.
    let refused = bob
        .call_raw(
            "send_message",
            json!({"to": "alice", "body": "hello?", "kind": "reply"}),
        )
        .await;
    assert!(error_text(&refused).contains("no open task"), "{refused}");

    // Two tasks so one stays open after the other's completion.
    let first = alice
        .call(
            "send_message",
            json!({"to": "bob", "body": "task one", "kind": "task"}),
        )
        .await;
    alice
        .call(
            "send_message",
            json!({"to": "bob", "body": "task two", "kind": "task"}),
        )
        .await;
    let done = bob
        .call(
            "complete_task",
            json!({"task_id": first["task_id"], "result": "done"}),
        )
        .await;

    // Answering the result explicitly is refused even while another task
    // keeps the pair's reply channel open.
    let thanks = bob
        .call_raw(
            "send_message",
            json!({"to": "alice", "body": "thanks!", "kind": "reply",
                   "ref": done["message_id"]}),
        )
        .await;
    assert!(error_text(&thanks).contains("task result"), "{thanks}");
}

#[tokio::test]
async fn a_finished_task_refuses_further_replies() {
    let (_p, mut clients) = project_with_bots(&["alice", "bob"]).await;
    let [alice, bob] = &mut clients[..] else {
        unreachable!()
    };

    let sent = alice
        .call(
            "send_message",
            json!({"to": "bob", "body": "quick job", "kind": "task"}),
        )
        .await;
    bob.call(
        "complete_task",
        json!({"task_id": sent["task_id"], "result": "done"}),
    )
    .await;

    // With the task closed there is no open task between the pair: a courtesy
    // acknowledgment has nowhere to land, in either direction.
    for (from, to) in [(&mut *bob, "alice"), (&mut *alice, "bob")] {
        let refused = from
            .call_raw(
                "send_message",
                json!({"to": to, "body": "thanks!", "kind": "reply"}),
            )
            .await;
        assert!(error_text(&refused).contains("no open task"), "{refused}");
    }
}

#[tokio::test]
async fn reserved_and_unknown_kinds_are_refused() {
    let (p, mut clients) = project_with_bots(&["alice", "bob"]).await;
    let alice = &mut clients[0];

    let chat = alice
        .call_raw(
            "send_message",
            json!({"to": "bob", "body": "hey", "kind": "chat"}),
        )
        .await;
    assert!(
        error_text(&chat).contains("reserved for user conversations"),
        "{chat}"
    );

    let done = alice
        .call_raw(
            "send_message",
            json!({"to": "bob", "body": "did it", "kind": "done"}),
        )
        .await;
    assert!(error_text(&done).contains("complete_task"), "{done}");

    let unknown = alice
        .call_raw(
            "send_message",
            json!({"to": "bob", "body": "x", "kind": "memo"}),
        )
        .await;
    assert!(error_text(&unknown).contains("unknown kind"), "{unknown}");

    // A note is fire-and-forget: delivered, but it opens no task.
    let note = alice
        .call(
            "send_message",
            json!({"to": "bob", "body": "FYI: deploy at noon", "kind": "note"}),
        )
        .await;
    assert!(note["task_id"].is_null(), "note opened a task: {note}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let bob = &mut clients[1];
    let inbox = bob.call("check_inbox", json!({})).await;
    let msgs = peer_messages(&inbox);
    assert_eq!(msgs.len(), 1, "{inbox}");
    assert_eq!(msgs[0]["kind"], "note");
    assert!(msgs[0]["task_id"].is_null(), "{inbox}");
    assert!(
        p.d.app
            .db
            .open_tasks_for(&p.ids[1])
            .expect("tasks")
            .is_empty(),
        "a note opened a task row"
    );
}

#[tokio::test]
async fn fan_out_is_capped_until_a_delegation_closes() {
    let (_p, mut clients) = project_with_bots(&["alice", "bob"]).await;
    let [alice, bob] = &mut clients[..] else {
        unreachable!()
    };

    let mut task_ids = Vec::new();
    for i in 0..MAX_TASK_FANOUT {
        let sent = alice
            .call(
                "send_message",
                json!({"to": "bob", "body": format!("job {i}"), "kind": "task"}),
            )
            .await;
        task_ids.push(sent["task_id"].as_str().expect("task id").to_string());
    }

    let refused = alice
        .call_raw(
            "send_message",
            json!({"to": "bob", "body": "job 4", "kind": "task"}),
        )
        .await;
    assert!(error_text(&refused).contains("open tasks"), "{refused}");
    // The refusal names what is holding the slots, so cancel_task is reachable
    // without the ids the original sends returned.
    for id in &task_ids {
        assert!(error_text(&refused).contains(id.as_str()), "{refused}");
    }
    assert!(error_text(&refused).contains("(to bob)"), "{refused}");

    // Closing one delegation frees the slot.
    bob.call(
        "complete_task",
        json!({"task_id": task_ids[0], "result": "done"}),
    )
    .await;
    alice
        .call(
            "send_message",
            json!({"to": "bob", "body": "job 4", "kind": "task"}),
        )
        .await;
}
