//! Workers and the project's shared repository. A worker clones, pulls and
//! pushes it itself, as its prompt says; placing it never waits on git. What
//! a worker leaves unpushed when it finishes is saved to its own branch. The
//! "remote" is a bare repository on disk, and the test plays the worker's
//! part in git. See `docs/workers.md`.

mod common;

use std::path::PathBuf;

use common::peers::{bot_named, pair, project, wait_until};
use common::repo::{book, complete, git, remote};
use common::tasks::{drain_all, drain_until};
use common::*;
use serde_json::json;

fn system_md(workspace: &str) -> String {
    let root = PathBuf::from(workspace);
    std::fs::read_to_string(root.parent().expect("bot dir").join("system.md")).expect("system.md")
}

/// The repository cannot even be reached, and still every spawn that fits
/// starts at once, with its task, and is told where to clone from.
#[tokio::test]
async fn spawning_never_waits_on_the_repository() {
    let d = spawn_daemon_with(|cfg| cfg.max_workers_per_project = 3).await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "novel").await;
    let unreachable = d._home.path().join("missing.git").display().to_string();
    let set = c
        .request(json!({ "type": "set_project_repo", "project_id": pid, "url": unreachable }))
        .await;
    assert_eq!(set["type"], "project", "{set}");
    let bot = create_bot(&mut c, &pid, "book").await;
    let token = d
        .app
        .secrets
        .bot_token(bot["id"].as_str().expect("id"))
        .expect("token");
    let mut bus = McpClient::new(&d, &token);

    for n in 1..=4 {
        let name = format!("ch-{n}");
        let spawned = bus
            .call(
                "spawn_worker",
                json!({ "name": name, "task": format!("Write {name}.") }),
            )
            .await;
        let expected = if n <= 3 { "running" } else { "queued" };
        assert_eq!(spawned["state"], expected, "{spawned}");
    }
    for n in 1..=3 {
        let worker = bot_named(&d, &pid, &format!("ch-{n}")).expect("worker");
        let prompt = system_md(&worker.workspace_path);
        assert!(
            prompt.contains(&format!("--branch main {unreachable} repo")),
            "{prompt}"
        );
        let token = d.app.secrets.bot_token(&worker.id).expect("token");
        let mut client = McpClient::new(&d, &token);
        drain_until(&mut client, &format!("Write ch-{n}.")).await;
    }
}

#[tokio::test]
async fn a_worker_that_pushes_its_own_work_retires_quietly() {
    let mut b = book().await;
    let (one, checkout, mut ch1) = b.spawn("ch-1").await;
    b.clone_as_worker(&checkout);
    std::fs::write(checkout.join("ch-1.md"), "It began.\n").expect("write");
    git(&checkout, &["add", "-A"]);
    git(&checkout, &["commit", "-q", "-m", "ch-1"]);
    git(&checkout, &["push", "-q", "origin", "HEAD:main"]);
    complete(&mut ch1, &one, "Chapter one, pushed").await;

    drain_until(&mut b.bus, "Chapter one, pushed").await;
    wait_until("ch-1 retires", || bot_named(&b.d, &b.pid, "ch-1").is_none()).await;
    let after = drain_all(&mut b.bus).await;
    assert!(
        !after.iter().any(|m| m["body"]
            .as_str()
            .is_some_and(|s| s.contains("saved on branch"))),
        "{after:?}"
    );
    assert_eq!(b.on_main("ch-1.md"), "It began.\n");
}

#[tokio::test]
async fn work_left_unpushed_after_finishing_is_saved_to_the_workers_branch() {
    let mut b = book().await;
    let (one, checkout, mut ch1) = b.spawn("ch-1").await;
    b.clone_as_worker(&checkout);
    std::fs::write(checkout.join("ch-1.md"), "Forgot to push.\n").expect("write");
    complete(&mut ch1, &one, "Chapter one").await;

    let seen = drain_until(&mut b.bus, "saved on branch").await;
    let note = seen
        .iter()
        .filter_map(|m| m["body"].as_str())
        .find(|s| s.contains("saved on branch"))
        .expect("salvage note");
    assert!(note.starts_with("ch-1 finished."), "{note}");
    let branch = note
        .split("saved on branch ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("branch");
    assert!(branch.starts_with("gravity/ch-1-"), "{branch}");
    assert_eq!(
        git(&b.origin, &["show", &format!("{branch}:ch-1.md")]),
        "Forgot to push.\n"
    );
    wait_until("ch-1 retires", || bot_named(&b.d, &b.pid, "ch-1").is_none()).await;
}

/// The Mac has no worker slots, so the worker runs on the linked PC, which
/// adopts the project's repository and tells its worker where it is.
#[tokio::test]
async fn a_worker_on_a_linked_machine_is_told_the_same_repository() {
    let remote_dir = tempfile::tempdir().expect("tempdir");
    let origin = remote(remote_dir.path()).display().to_string();
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
        .request(json!({ "type": "set_project_repo", "project_id": pid, "url": origin }))
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
    assert_eq!(spawned["state"], "running", "{spawned}");
    assert_eq!(spawned["machine"], "win", "{spawned}");
    let worker = bot_named(&win, &win_pid, "ch-1").expect("ch-1 runs on the PC");
    assert!(system_md(&worker.workspace_path).contains(&origin));
    assert_eq!(
        win.app
            .db
            .project_repo(&win_pid)
            .expect("db")
            .map(|r| r.url),
        Some(origin)
    );
}
