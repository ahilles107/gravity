//! The worker queue and the retirement of finished temporary bots.

use bus::*;
use chrono::{Duration, Utc};

use super::workers::{NewWorker, UNTASKED_GRACE_MINUTES};
use super::Db;

fn worker<'a>(project: &'a str, parent: &'a str, name: &'a str) -> NewWorker<'a> {
    NewWorker {
        project_id: project,
        parent_bot_id: parent,
        name,
        brief: "write chapter",
        description: "",
        instructions: "",
        runtime: None,
        machine: None,
        deadline_hours: 24,
    }
}

#[test]
fn queue_is_fifo_and_reserves_names() {
    let db = Db::open_in_memory().unwrap();
    let p = db.create_project("Book", "book").unwrap();
    let lead = db
        .create_bot(&p.id, "lead", "", "", "", "/tmp/w", "lead", None)
        .unwrap();
    let a = db.insert_worker(&worker(&p.id, &lead.id, "ch-1")).unwrap();
    let b = db.insert_worker(&worker(&p.id, &lead.id, "ch-2")).unwrap();
    assert!(db.worker_name_reserved(&p.id, "CH-1").unwrap());
    let queue = db.queued_workers(&p.id).unwrap();
    assert_eq!(
        queue.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(),
        ["ch-1", "ch-2"]
    );
    assert_eq!(db.queue_position(&b).unwrap(), Some(2));

    assert!(db
        .finish_worker(&a.id, WorkerState::Cancelled, None)
        .unwrap());
    assert!(!db.finish_worker(&a.id, WorkerState::Done, None).unwrap());
    let b = db.get_worker(&b.id).unwrap().unwrap();
    assert_eq!(db.queue_position(&b).unwrap(), Some(1));
    assert!(!db.worker_name_reserved(&p.id, "ch-1").unwrap());
}

#[test]
fn workers_do_not_count_against_the_bot_cap() {
    let db = Db::open_in_memory().unwrap();
    let p = db.create_project("Book", "book").unwrap();
    let lead = db
        .create_bot(&p.id, "lead", "", "", "", "/tmp/w", "lead", None)
        .unwrap();
    let temp = db
        .create_bot(&p.id, "ch-1", "", "", "", "/tmp/c", "ch-1", None)
        .unwrap();
    db.set_bot_temporary(&temp.id, true).unwrap();
    assert_eq!(db.count_live_bots(&p.id).unwrap(), 1);
    assert_eq!(db.count_live_workers(&p.id).unwrap(), 1);
    assert!(db.get_bot(&temp.id).unwrap().unwrap().temporary);
    assert!(!db.get_bot(&lead.id).unwrap().unwrap().temporary);
}

#[test]
fn a_temporary_bot_finishes_once_its_task_closes() {
    let db = Db::open_in_memory().unwrap();
    let p = db.create_project("Book", "book").unwrap();
    let lead = db
        .create_bot(&p.id, "lead", "", "", "", "/tmp/w", "lead", None)
        .unwrap();
    let temp = db
        .create_bot(&p.id, "ch-1", "", "", "", "/tmp/c", "ch-1", None)
        .unwrap();
    db.set_bot_temporary(&temp.id, true).unwrap();
    let soon = Utc::now();
    // Fresh and untasked: still inside its grace period.
    assert!(db.finished_temporary_bots(soon).unwrap().is_empty());

    let sender = Sender {
        kind: SenderKind::Bot,
        bot_id: Some(lead.id.clone()),
        name: lead.name.clone(),
    };
    let conv = db.dm_conversation(&temp.id).unwrap().unwrap();
    let msg = db
        .insert_message(&conv.id, &sender, MessageKind::Task, "go", None, None)
        .unwrap();
    let task = db
        .create_task(&msg.id, Some(&lead.id), &temp.id, None, 1, &lead.id)
        .unwrap();
    assert!(db.finished_temporary_bots(soon).unwrap().is_empty());

    db.try_close_task(&task.id, TaskState::Done).unwrap();
    let done = db.finished_temporary_bots(soon).unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].id, temp.id);

    // Long untasked bots are retired too.
    let idle = db
        .create_bot(&p.id, "ch-2", "", "", "", "/tmp/d", "ch-2", None)
        .unwrap();
    db.set_bot_temporary(&idle.id, true).unwrap();
    let later = Utc::now() + Duration::minutes(UNTASKED_GRACE_MINUTES + 1);
    assert_eq!(db.finished_temporary_bots(later).unwrap().len(), 2);
}
