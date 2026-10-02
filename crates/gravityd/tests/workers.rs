//! Temporary workers: spawned for one task, queued past the worker cap,
//! retired when their task closes, and placed on a linked machine when this
//! one is full. See `docs/workers.md`.

mod common;

use bus::WorkerState;
use common::peers::{bot_named, pair, project, wait_until};
use common::tasks::{drain_until, error_text};
use common::*;
use serde_json::{json, Value};

/// A project whose only bot, `book`, fills the bot cap on its own, with room
/// for `workers` workers.
async fn book(workers: usize) -> (TestDaemon, String, McpClient) {
    let d = spawn_daemon_with(|cfg| {
        cfg.max_bots_per_project = 1;
        cfg.max_workers_per_project = workers;
    })
    .await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "novel").await;
    let bot = create_bot(&mut c, &pid, "book").await;
    let token = d
        .app
        .secrets
        .bot_token(bot["id"].as_str().expect("id"))
        .expect("token");
    let client = McpClient::new(&d, &token);
    (d, pid, client)
}

async fn spawn(bus: &mut McpClient, name: &str) -> Value {
    bus.call(
        "spawn_worker",
        json!({ "name": name, "task": format!("Write {name}.") }),
    )
    .await
}

/// An MCP client for the worker a spawn started.
fn worker_client(d: &TestDaemon, pid: &str, name: &str) -> McpClient {
    let bot = bot_named(d, pid, name).expect("worker bot");
    assert!(bot.temporary, "{name} should be a worker");
    let token = d.app.secrets.bot_token(&bot.id).expect("token");
    McpClient::new(d, &token)
}

fn worker_state(d: &TestDaemon, parent: &str, name: &str) -> Option<WorkerState> {
    d.app
        .db
        .workers_of(parent, 50)
        .expect("workers")
        .into_iter()
        .find(|w| w.name == name)
        .map(|w| w.state)
}

fn book_id(d: &TestDaemon, pid: &str) -> String {
    bot_named(d, pid, "book").expect("book").id
}

#[tokio::test]
async fn spawns_past_the_cap_wait_and_start_as_workers_finish() {
    let (d, pid, mut bus) = book(2).await;

    // The bot cap is full with `book` alone; workers have their own.
    let first = spawn(&mut bus, "ch-1").await;
    assert_eq!(first["state"], "running", "{first}");
    assert_eq!(first["machine"], "here");
    assert_eq!(spawn(&mut bus, "ch-2").await["state"], "running");
    let third = spawn(&mut bus, "ch-3").await;
    assert_eq!(third["state"], "queued", "{third}");
    assert_eq!(third["queue_position"], 1);
    // Workers hold neither bot slots nor the three-task fan-out.
    assert_eq!(d.app.db.count_live_bots(&pid).expect("count"), 1);
    assert_eq!(spawn(&mut bus, "ch-4").await["queue_position"], 2);

    // A queued worker's name is reserved.
    let clash = bus
        .call_raw("spawn_worker", json!({ "name": "ch-3", "task": "again" }))
        .await;
    assert!(error_text(&clash).contains("already exists"), "{clash}");

    // ch-1 finishes: its result reaches book, it is retired, ch-3 starts.
    let mut ch1 = worker_client(&d, &pid, "ch-1");
    let task_id = first["task_id"].as_str().expect("task id");
    ch1.call(
        "complete_task",
        json!({ "task_id": task_id, "result": "chapter one written" }),
    )
    .await;
    let seen = drain_until(&mut bus, "chapter one written").await;
    assert!(
        seen.iter().any(|m| m["kind"] == "done"),
        "book never got the result: {seen:?}"
    );
    let parent = book_id(&d, &pid);
    wait_until("ch-1 is retired", || bot_named(&d, &pid, "ch-1").is_none()).await;
    wait_until("ch-3 starts in its slot", || {
        worker_state(&d, &parent, "ch-3") == Some(WorkerState::Running)
    })
    .await;
    assert_eq!(worker_state(&d, &parent, "ch-1"), Some(WorkerState::Done));
    assert_eq!(worker_state(&d, &parent, "ch-4"), Some(WorkerState::Queued));

    let listed = bus.call("list_workers", json!({})).await;
    assert_eq!(listed["running_here"], 2, "{listed}");
    assert_eq!(listed["max_workers_here"], 2);
    let names: Vec<_> = listed["workers"]
        .as_array()
        .expect("workers")
        .iter()
        .map(|w| (w["name"].clone(), w["state"].clone()))
        .collect();
    assert!(
        names.contains(&(json!("ch-4"), json!("queued"))),
        "{listed}"
    );
    assert!(names.contains(&(json!("ch-1"), json!("done"))), "{listed}");
}

#[tokio::test]
async fn cancelling_drops_a_queued_spawn_and_retires_a_running_one() {
    let (d, pid, mut bus) = book(1).await;
    spawn(&mut bus, "ch-1").await;
    assert_eq!(spawn(&mut bus, "ch-2").await["state"], "queued");
    let parent = book_id(&d, &pid);

    let dropped = bus.call("cancel_worker", json!({ "name": "ch-2" })).await;
    assert_eq!(dropped["state"], "cancelled", "{dropped}");

    let cancelled = bus
        .call(
            "cancel_worker",
            json!({ "name": "ch-1", "reason": "plot changed" }),
        )
        .await;
    assert_eq!(cancelled["state"], "cancelled", "{cancelled}");
    wait_until("ch-1 is retired", || bot_named(&d, &pid, "ch-1").is_none()).await;
    // Nothing was left queued to take the slot.
    assert_eq!(
        worker_state(&d, &parent, "ch-2"),
        Some(WorkerState::Cancelled)
    );
    assert_eq!(d.app.db.count_live_workers(&pid).expect("count"), 0);
}

#[tokio::test]
async fn workers_cannot_spawn_workers_and_bad_machines_are_refused() {
    let (d, pid, mut bus) = book(2).await;
    spawn(&mut bus, "ch-1").await;
    let mut ch1 = worker_client(&d, &pid, "ch-1");
    let nested = ch1
        .call_raw("spawn_worker", json!({ "task": "help me" }))
        .await;
    assert!(
        error_text(&nested).contains("workers cannot spawn"),
        "{nested}"
    );

    let nowhere = bus
        .call_raw(
            "spawn_worker",
            json!({ "task": "x", "machine": "atlantis" }),
        )
        .await;
    assert!(
        error_text(&nowhere).contains("no machine named"),
        "{nowhere}"
    );

    // A spawn without a name gets the first free worker-N.
    let unnamed = bus.call("spawn_worker", json!({ "task": "Index." })).await;
    assert_eq!(unnamed["name"], "worker-1", "{unnamed}");
}

#[tokio::test]
async fn deleting_a_running_worker_closes_its_spawn() {
    let (d, pid, mut bus) = book(1).await;
    let started = spawn(&mut bus, "ch-1").await;
    let parent = book_id(&d, &pid);
    // The owner deletes the worker mid-task: its spawn closes as cancelled.
    let bot = bot_named(&d, &pid, "ch-1").expect("worker");
    gravityd::botmgmt::archive_bot(&d.app, &bot, &gravityd::db::Actor::User, None)
        .expect("archive");
    wait_until("the spawn settles", || {
        worker_state(&d, &parent, "ch-1") == Some(WorkerState::Cancelled)
    })
    .await;
    assert!(started["task_id"].is_string());
}

/// The Mac runs one worker at a time; the PC, linked into the same project,
/// takes the next spawn instead of leaving it queued.
#[tokio::test]
async fn a_full_machine_hands_spawns_to_a_linked_one() {
    let mac = spawn_daemon_with(|cfg| {
        cfg.user_home = cfg.home.join("user");
        cfg.max_workers_per_project = 1;
    })
    .await;
    let win = spawn_daemon_with(|cfg| {
        cfg.user_home = cfg.home.join("user");
        cfg.max_workers_per_project = 1;
    })
    .await;
    let mut mac_client = WsClient::connect(&mac).await;
    let mut win_client = WsClient::connect(&win).await;
    let (win_peer, mac_peer) = pair(&mac, &win, &mut mac_client, &mut win_client).await;
    let mac_app = project(&mut mac_client, "novel").await;
    let lead = create_bot(&mut mac_client, &mac_app, "book").await;
    let linked = mac_client
        .request(json!({
            "type": "link_project", "project_id": mac_app, "peer_id": mac_peer
        }))
        .await;
    assert_eq!(linked["type"], "project", "{linked}");
    let win_app = win
        .app
        .db
        .project_link_by_remote(&win_peer, &mac_app)
        .expect("db")
        .expect("the PC holds its half of the link")
        .project_id;
    let token = mac
        .app
        .secrets
        .bot_token(lead["id"].as_str().expect("id"))
        .expect("token");
    let mut bus = McpClient::new(&mac, &token);

    let here = spawn(&mut bus, "ch-1").await;
    assert_eq!(here["machine"], "here", "{here}");
    let there = spawn(&mut bus, "ch-2").await;
    assert_eq!(there["state"], "running", "{there}");
    assert_eq!(there["machine"], "win", "{there}");
    let waiting = spawn(&mut bus, "ch-3").await;
    assert_eq!(waiting["state"], "queued", "{waiting}");

    // The worker runs on the PC as a worker there, under its own cap.
    let remote = bot_named(&win, &win_app, "ch-2").expect("ch-2 runs on the PC");
    assert!(remote.temporary);
    assert_eq!(win.app.db.count_live_workers(&win_app).expect("count"), 1);
    let stand_in = bot_named(&mac, &mac_app, "ch-2").expect("ch-2 stands in on the Mac");
    assert!(stand_in.temporary && stand_in.peer_id.is_some());

    // It finishes on the PC; the result crosses, and both sides retire it.
    let token = win.app.secrets.bot_token(&remote.id).expect("token");
    let mut ch2 = McpClient::new(&win, &token);
    let inbox = drain_until(&mut ch2, "Write ch-2").await;
    let task_id = inbox
        .iter()
        .find_map(|m| m["task_id"].as_str())
        .expect("ch-2 got its task")
        .to_string();
    ch2.call(
        "complete_task",
        json!({ "task_id": task_id, "result": "chapter two written" }),
    )
    .await;
    drain_until(&mut bus, "chapter two written").await;
    wait_until("ch-2 retires on the PC", || {
        bot_named(&win, &win_app, "ch-2").is_none()
    })
    .await;
    wait_until("its stand-in goes too", || {
        bot_named(&mac, &mac_app, "ch-2").is_none()
    })
    .await;
    let parent = lead["id"].as_str().expect("id");
    wait_until("ch-3 takes the PC's freed slot", || {
        worker_state(&mac, parent, "ch-3") == Some(WorkerState::Running)
    })
    .await;
    assert_eq!(worker_state(&mac, parent, "ch-2"), Some(WorkerState::Done));
}

/// The app lists a project's queue, hears when it changes, and can cancel a
/// spawn on the owner's behalf.
#[tokio::test]
async fn the_owner_sees_the_queue_and_can_cancel_from_the_app() {
    let d = spawn_daemon_with(|cfg| cfg.max_workers_per_project = 1).await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "novel").await;
    let bot = create_bot(&mut c, &pid, "book").await;
    let token = d
        .app
        .secrets
        .bot_token(bot["id"].as_str().expect("id"))
        .expect("token");
    let mut bus = McpClient::new(&d, &token);

    spawn(&mut bus, "ch-1").await;
    c.wait_for(|v| v["type"] == "workers_updated" && v["project_id"] == pid.as_str())
        .await;
    spawn(&mut bus, "ch-2").await;

    let listed = c
        .request(json!({ "type": "list_workers", "project_id": pid }))
        .await;
    assert_eq!(listed["type"], "workers", "{listed}");
    assert_eq!(listed["running_here"], 1);
    assert_eq!(listed["max_workers_here"], 1);
    let workers = listed["workers"].as_array().expect("workers");
    assert_eq!(workers[0]["name"], "ch-1");
    assert_eq!(workers[0]["state"], "running");
    assert_eq!(workers[0]["parent_name"], "book");
    assert_eq!(workers[0]["brief"], "Write ch-1.");
    assert_eq!(workers[1]["name"], "ch-2");
    assert_eq!(workers[1]["queue_position"], 1);

    let cancelled = c
        .request(json!({ "type": "cancel_worker", "worker_id": workers[1]["id"] }))
        .await;
    assert_eq!(cancelled["type"], "worker", "{cancelled}");
    assert_eq!(cancelled["worker"]["state"], "cancelled");
    let again = c
        .request(json!({ "type": "cancel_worker", "worker_id": workers[1]["id"] }))
        .await;
    assert_eq!(again["type"], "error", "{again}");

    // Cancelling the running one tells it to stop, as the owner.
    c.request(json!({ "type": "cancel_worker", "worker_id": workers[0]["id"] }))
        .await;
    wait_until("ch-1 retires", || bot_named(&d, &pid, "ch-1").is_none()).await;
}

/// The daemon stopped between creating a worker and giving it its task: the
/// next pass gives the task to that bot instead of failing over its name.
#[tokio::test]
async fn a_worker_left_without_its_task_gets_it() {
    let (d, pid, _bus) = book(2).await;
    let parent = bot_named(&d, &pid, "book").expect("book");
    let queued = d
        .app
        .db
        .insert_worker(&gravityd::db::NewWorker {
            project_id: &pid,
            parent_bot_id: &parent.id,
            name: "ch-1",
            brief: "Write ch-1.",
            description: "",
            instructions: "",
            runtime: None,
            machine: None,
            deadline_hours: 24,
        })
        .expect("queue");
    let actor = gravityd::db::Actor::Bot {
        id: &parent.id,
        project_id: &pid,
    };
    let stranded = gravityd::botmgmt::create_worker_bot(
        &d.app,
        &pid,
        &gravityd::botmgmt::IdentityEdit {
            name: Some("ch-1"),
            ..Default::default()
        },
        Some(&parent),
        &actor,
        parent.runtime,
    )
    .expect("worker bot")
    .bot;

    d.app.workers.nudge();
    wait_until("the stranded worker is given its task", || {
        d.app
            .db
            .get_worker(&queued.id)
            .expect("db")
            .is_some_and(|w| {
                w.state == WorkerState::Running && w.bot_id.as_deref() == Some(stranded.id.as_str())
            })
    })
    .await;
    let token = d.app.secrets.bot_token(&stranded.id).expect("token");
    drain_until(&mut McpClient::new(&d, &token), "Write ch-1.").await;
}

#[tokio::test]
async fn deleting_a_bot_twice_deletes_it_once() {
    let (d, pid, _bus) = book(1).await;
    let bot = bot_named(&d, &pid, "book").expect("book");
    let user = gravityd::db::Actor::User;
    gravityd::botmgmt::archive_bot(&d.app, &bot, &user, None).expect("first");
    gravityd::botmgmt::archive_bot(&d.app, &bot, &user, None).expect("second");
    let archived = d.app.db.get_bot(&bot.id).expect("db").expect("row");
    assert_eq!(archived.name.matches('#').count(), 1, "{}", archived.name);
}
