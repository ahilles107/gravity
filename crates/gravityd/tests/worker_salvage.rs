//! Work a worker never pushed — its task was cancelled or expired — is saved
//! to the worker's own branch before it retires, and its parent is told
//! where. See "Shared repository" in `docs/workers.md`.

mod common;

use common::peers::{bot_named, wait_until};
use common::repo::{book, git};
use common::tasks::{drain_all, drain_until};
use serde_json::{json, Value};

/// The branch a salvage note names.
fn saved_branch(seen: &[Value]) -> String {
    seen.iter()
        .filter_map(|m| m["body"].as_str())
        .find_map(|body| body.split("saved on branch ").nth(1))
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("no salvage note in {seen:?}"))
        .to_string()
}

#[tokio::test]
async fn a_cancelled_workers_work_is_saved_to_its_own_branch() {
    let mut b = book().await;
    let (_spawned, checkout, _) = b.spawn("ch-1").await;
    b.clone_as_worker(&checkout);
    std::fs::create_dir_all(checkout.join("chapters")).expect("dir");
    std::fs::write(checkout.join("chapters/1.md"), "Half a chapter.\n").expect("write");

    b.bus
        .call(
            "cancel_worker",
            json!({ "name": "ch-1", "reason": "plot changed" }),
        )
        .await;
    let seen = drain_until(&mut b.bus, "saved on branch").await;
    let note = seen
        .iter()
        .find(|m| {
            m["body"]
                .as_str()
                .is_some_and(|s| s.contains("saved on branch"))
        })
        .expect("salvage note");
    assert_eq!(note["kind"], "note");
    assert!(
        note["body"]
            .as_str()
            .is_some_and(|s| s.contains("ch-1 stopped: its task was cancelled")),
        "{note}"
    );
    let branch = saved_branch(&seen);
    assert_eq!(
        git(&b.origin, &["show", &format!("{branch}:chapters/1.md")]),
        "Half a chapter.\n"
    );
    // Nothing unfinished reaches the shared branch.
    let main = git(&b.origin, &["ls-tree", "-r", "--name-only", "main"]);
    assert!(!main.contains("chapters/1.md"), "{main}");

    wait_until("ch-1 retires", || bot_named(&b.d, &b.pid, "ch-1").is_none()).await;
}

#[tokio::test]
async fn an_expired_workers_work_is_saved_too() {
    let mut b = book().await;
    let (spawned, checkout, _) = b.spawn("ch-1").await;
    b.clone_as_worker(&checkout);
    std::fs::write(checkout.join("notes.md"), "Outline.\n").expect("write");
    let task_id = spawned["task_id"].as_str().expect("task id");
    assert!(b.d.app.db.expire_task(task_id).expect("expire"));
    b.d.app.workers.nudge();

    let seen = drain_until(&mut b.bus, "saved on branch").await;
    assert!(
        seen.iter().any(|m| m["body"]
            .as_str()
            .is_some_and(|s| s.contains("its task was expired"))),
        "{seen:?}"
    );
    let branch = saved_branch(&seen);
    assert_eq!(
        git(&b.origin, &["show", &format!("{branch}:notes.md")]),
        "Outline.\n"
    );
    wait_until("ch-1 retires", || bot_named(&b.d, &b.pid, "ch-1").is_none()).await;
}

#[tokio::test]
async fn a_worker_with_nothing_unpushed_retires_quietly() {
    let mut b = book().await;
    let (_spawned, checkout, _) = b.spawn("ch-1").await;
    b.clone_as_worker(&checkout);
    b.bus.call("cancel_worker", json!({ "name": "ch-1" })).await;
    wait_until("ch-1 retires", || bot_named(&b.d, &b.pid, "ch-1").is_none()).await;
    let seen = drain_all(&mut b.bus).await;
    assert!(
        !seen.iter().any(|m| m["body"]
            .as_str()
            .is_some_and(|s| s.contains("saved on branch"))),
        "{seen:?}"
    );
}
