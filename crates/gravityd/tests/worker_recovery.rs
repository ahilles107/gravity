//! Recovering from the daemon stopping mid-placement, and deleting twice.

mod common;

use bus::WorkerState;
use common::peers::{bot_named, project, wait_until};
use common::tasks::drain_until;
use common::*;

/// A project whose only bot is `book`, with room for `workers` workers.
async fn book(workers: usize) -> (TestDaemon, String) {
    let d = spawn_daemon_with(|cfg| cfg.max_workers_per_project = workers).await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "novel").await;
    create_bot(&mut c, &pid, "book").await;
    (d, pid)
}

/// The daemon stopped between creating a worker and giving it its task: the
/// next pass gives the task to that bot instead of failing over its name.
#[tokio::test]
async fn a_worker_left_without_its_task_gets_it() {
    let (d, pid) = book(2).await;
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
    let (d, pid) = book(1).await;
    let bot = bot_named(&d, &pid, "book").expect("book");
    let user = gravityd::db::Actor::User;
    gravityd::botmgmt::archive_bot(&d.app, &bot, &user, None).expect("first");
    gravityd::botmgmt::archive_bot(&d.app, &bot, &user, None).expect("second");
    let archived = d.app.db.get_bot(&bot.id).expect("db").expect("row");
    assert_eq!(archived.name.matches('#').count(), 1, "{}", archived.name);
}
