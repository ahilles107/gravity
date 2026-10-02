//! Workers and the project's shared repository: each starts from a fresh
//! checkout of the branch tip, and its work is pushed there when it
//! completes — or to its own branch when that conflicts. The "remote" is a
//! bare repository on disk. See `docs/workers.md`.

mod common;

use std::path::PathBuf;

use common::peers::{bot_named, pair, project, wait_until};
use common::repo::{book, complete, git, remote};
use common::tasks::drain_until;
use common::*;
use serde_json::json;

#[tokio::test]
async fn workers_start_from_the_tip_and_push_their_work_back() {
    let mut b = book().await;
    let (one, one_dir, mut ch1) = b.spawn("ch-1").await;
    let (two, two_dir, mut ch2) = b.spawn("ch-2").await;
    assert!(one_dir.join("README.md").is_file(), "ch-1 has a checkout");

    // Both write their own chapter from the same starting point.
    std::fs::create_dir_all(one_dir.join("chapters")).expect("dir");
    std::fs::write(one_dir.join("chapters/1.md"), "It began.\n").expect("write");
    std::fs::create_dir_all(two_dir.join("chapters")).expect("dir");
    std::fs::write(two_dir.join("chapters/2.md"), "It went on.\n").expect("write");

    complete(&mut ch1, &one, "Chapter one").await;
    complete(&mut ch2, &two, "Chapter two").await;
    let seen = drain_until(&mut b.bus, "Chapter two").await;
    let reports: Vec<&str> = seen
        .iter()
        .filter(|m| m["kind"] == "done")
        .filter_map(|m| m["body"].as_str())
        .collect();
    assert_eq!(reports.len(), 2, "{seen:?}");
    assert!(
        reports.iter().all(|r| r.contains("repo: pushed")),
        "{reports:?}"
    );
    // The second rebased onto the first: main holds both.
    assert_eq!(b.on_main("chapters/1.md"), "It began.\n");
    assert_eq!(b.on_main("chapters/2.md"), "It went on.\n");

    // A worker spawned now starts from the pushed work.
    let (_three, three_dir, _) = b.spawn("ch-3").await;
    assert!(three_dir.join("chapters/1.md").is_file());
    assert!(three_dir.join("chapters/2.md").is_file());
    // A retired worker's checkout goes once everything in it is pushed.
    wait_until("ch-1's checkout is removed", || !one_dir.exists()).await;
}

/// The Mac has no worker slots, so the worker runs on the linked PC: it
/// clones the repository there, with the PC's own git, and pushes to it.
#[tokio::test]
async fn a_worker_on_a_linked_machine_checks_out_and_pushes_the_same_repo() {
    let remote_dir = tempfile::tempdir().expect("tempdir");
    let origin = remote(remote_dir.path());
    let mac = spawn_daemon_with(|cfg| {
        cfg.user_home = cfg.home.join("user");
        cfg.max_workers_per_project = 0;
    })
    .await;
    let win = spawn_daemon_with(|cfg| cfg.user_home = cfg.home.join("user")).await;
    let mut mac_client = WsClient::connect(&mac).await;
    let mut win_client = WsClient::connect(&win).await;
    let (win_peer, mac_peer) = pair(&mac, &win, &mut mac_client, &mut win_client).await;
    let pid = project(&mut mac_client, "novel").await;
    mac_client
        .request(json!({
            "type": "set_project_repo", "project_id": pid,
            "url": origin.display().to_string()
        }))
        .await;
    let lead = create_bot(&mut mac_client, &pid, "book").await;
    let linked = mac_client
        .request(json!({ "type": "link_project", "project_id": pid, "peer_id": mac_peer }))
        .await;
    assert_eq!(linked["type"], "project", "{linked}");
    let win_pid = win
        .app
        .db
        .project_link_by_remote(&win_peer, &pid)
        .expect("db")
        .expect("linked")
        .project_id;
    let token = mac
        .app
        .secrets
        .bot_token(lead["id"].as_str().expect("id"))
        .expect("token");
    let mut bus = McpClient::new(&mac, &token);

    let spawned = bus
        .call(
            "spawn_worker",
            json!({ "name": "ch-1", "task": "Write ch-1." }),
        )
        .await;
    assert_eq!(spawned["machine"], "win", "{spawned}");
    let worker = bot_named(&win, &win_pid, "ch-1").expect("ch-1 runs on the PC");
    let checkout = PathBuf::from(&worker.workspace_path).join("repo");
    assert!(checkout.join("README.md").is_file(), "cloned on the PC");
    // The PC adopted the project's repository for its half of the link.
    assert!(win.app.db.project_repo(&win_pid).expect("db").is_some());

    std::fs::write(checkout.join("ch-1.md"), "From the PC.\n").expect("write");
    let token = win.app.secrets.bot_token(&worker.id).expect("token");
    let mut ch1 = McpClient::new(&win, &token);
    let inbox = drain_until(&mut ch1, "Write ch-1").await;
    let task_id = inbox
        .iter()
        .find_map(|m| m["task_id"].as_str())
        .expect("task id")
        .to_string();
    ch1.call(
        "complete_task",
        json!({ "task_id": task_id, "result": "Done on the PC" }),
    )
    .await;
    let seen = drain_until(&mut bus, "Done on the PC").await;
    assert!(
        seen.iter().any(|m| m["body"]
            .as_str()
            .is_some_and(|s| s.contains("repo: pushed"))),
        "{seen:?}"
    );
    assert_eq!(git(&origin, &["show", "main:ch-1.md"]), "From the PC.\n");
}

#[tokio::test]
async fn conflicting_work_goes_to_the_workers_own_branch() {
    let mut b = book().await;
    let (one, one_dir, mut ch1) = b.spawn("ch-1").await;
    let (two, two_dir, mut ch2) = b.spawn("ch-2").await;
    std::fs::write(one_dir.join("README.md"), "# The Long Book\n").expect("write");
    std::fs::write(two_dir.join("README.md"), "# The Short Book\n").expect("write");

    complete(&mut ch1, &one, "Retitled long").await;
    complete(&mut ch2, &two, "Retitled short").await;
    let seen = drain_until(&mut b.bus, "Retitled short").await;
    let short = seen
        .iter()
        .find(|m| {
            m["body"]
                .as_str()
                .is_some_and(|s| s.contains("Retitled short"))
        })
        .and_then(|m| m["body"].as_str())
        .expect("ch-2's result");
    assert!(short.contains("conflicts with main"), "{short}");

    assert_eq!(b.on_main("README.md"), "# The Long Book\n");
    let branch = short
        .split("pushed to branch ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("the branch is named");
    assert_eq!(
        git(&b.origin, &["show", &format!("{branch}:README.md")]),
        "# The Short Book\n"
    );
}

#[tokio::test]
async fn a_worker_with_nothing_to_push_says_so() {
    let mut b = book().await;
    let (one, _, mut ch1) = b.spawn("ch-1").await;
    complete(&mut ch1, &one, "Nothing needed").await;
    let seen = drain_until(&mut b.bus, "Nothing needed").await;
    assert!(
        seen.iter().any(|m| m["body"]
            .as_str()
            .is_some_and(|s| s.contains("no changes to push"))),
        "{seen:?}"
    );
}

#[tokio::test]
async fn an_unreachable_repository_fails_the_spawn_and_says_why() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "novel").await;
    let set = c
        .request(json!({
            "type": "set_project_repo", "project_id": pid,
            "url": d._home.path().join("missing.git").display().to_string()
        }))
        .await;
    assert_eq!(set["type"], "project", "{set}");
    let bad = c
        .request(json!({
            "type": "set_project_repo", "project_id": pid, "url": "--upload-pack=x"
        }))
        .await;
    assert_eq!(bad["type"], "error", "{bad}");

    let bot = create_bot(&mut c, &pid, "book").await;
    let token = d
        .app
        .secrets
        .bot_token(bot["id"].as_str().expect("id"))
        .expect("token");
    let mut bus = McpClient::new(&d, &token);
    let spawned = bus.call("spawn_worker", json!({ "task": "x" })).await;
    assert_eq!(spawned["state"], "failed", "{spawned}");
    assert!(
        spawned["note"]
            .as_str()
            .is_some_and(|n| n.contains("cloning")),
        "{spawned}"
    );
    assert!(
        bot_named(&d, &pid, "worker-1").is_none(),
        "the worker was retired"
    );
}
