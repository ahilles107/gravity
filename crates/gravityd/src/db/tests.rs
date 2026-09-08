use super::*;

fn user_sender() -> Sender {
    Sender {
        kind: SenderKind::User,
        bot_id: None,
        name: "user".to_string(),
    }
}

pub(super) fn setup() -> (Db, Bot) {
    let db = Db::open_in_memory().unwrap();
    let p = db.create_project("test", "test").unwrap();
    let bot = db
        .create_bot(
            &p.id,
            "alice",
            "test bot",
            "",
            "",
            "/tmp/alice",
            "alice",
            None,
        )
        .unwrap();
    (db, bot)
}

#[test]
fn migrations_apply_twice() {
    let db = Db::open_in_memory().unwrap();
    db.migrate().unwrap();
    assert!(db.integrity_check().unwrap());
}

#[test]
fn delivery_is_idempotent_on_key() {
    let (db, bot) = setup();
    let conv = db.dm_conversation(&bot.id).unwrap().unwrap();
    let msg = db
        .insert_message(&conv.id, &user_sender(), MessageKind::Chat, "hi", None)
        .unwrap();
    let d1 = db.enqueue_delivery(&msg.id, &bot.id, "key-1").unwrap();
    let d2 = db.enqueue_delivery(&msg.id, &bot.id, "key-1").unwrap();
    assert_eq!(d1.id, d2.id);
}

#[test]
fn lease_and_retry_flow() {
    let (db, bot) = setup();
    let conv = db.dm_conversation(&bot.id).unwrap().unwrap();
    let msg = db
        .insert_message(&conv.id, &user_sender(), MessageKind::Task, "do it", None)
        .unwrap();
    db.enqueue_delivery(&msg.id, &bot.id, "k").unwrap();

    let leased = db.lease_due_deliveries(60, 10).unwrap();
    assert_eq!(leased.len(), 1);
    // Not visible while leased.
    assert!(db.lease_due_deliveries(60, 10).unwrap().is_empty());

    let state = db
        .mark_delivery_retry(&leased[0].id, "runtime busy", now(), 10)
        .unwrap();
    assert_eq!(state, DeliveryState::Queued);
    let again = db.lease_due_deliveries(60, 10).unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].attempt_count, 1);

    db.mark_delivered(&again[0].id).unwrap();
    assert_eq!(db.unread_count(&bot.id).unwrap(), 1);
    let inbox = db.consume_inbox(&bot.id).unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(db.unread_count(&bot.id).unwrap(), 0);
    let d = db.get_delivery(&again[0].id).unwrap().unwrap();
    assert_eq!(d.state, DeliveryState::Acknowledged);
}

#[test]
fn delivery_fails_after_max_attempts() {
    let (db, bot) = setup();
    let conv = db.dm_conversation(&bot.id).unwrap().unwrap();
    let msg = db
        .insert_message(&conv.id, &user_sender(), MessageKind::Task, "x", None)
        .unwrap();
    db.enqueue_delivery(&msg.id, &bot.id, "k").unwrap();
    for _ in 0..3 {
        let leased = db.lease_due_deliveries(60, 10).unwrap();
        if leased.is_empty() {
            break;
        }
        db.mark_delivery_retry(&leased[0].id, "boom", now(), 3)
            .unwrap();
    }
    let failed = db
        .list_deliveries(Some(&bot.id), Some(DeliveryState::Failed))
        .unwrap();
    assert_eq!(failed.len(), 1);
    assert!(db.retry_failed_delivery(&failed[0].id).unwrap());
    let queued = db
        .list_deliveries(Some(&bot.id), Some(DeliveryState::Queued))
        .unwrap();
    assert_eq!(queued.len(), 1);
}

#[test]
fn fts_search_finds_messages() {
    let (db, bot) = setup();
    let conv = db.dm_conversation(&bot.id).unwrap().unwrap();
    db.insert_message(
        &conv.id,
        &user_sender(),
        MessageKind::Chat,
        "quarterly revenue report",
        None,
    )
    .unwrap();
    db.insert_message(
        &conv.id,
        &user_sender(),
        MessageKind::Chat,
        "unrelated",
        None,
    )
    .unwrap();
    let hits = db.search_messages("revenue", 10).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn task_hop_limit_enforced() {
    let (db, bot) = setup();
    let conv = db.dm_conversation(&bot.id).unwrap().unwrap();
    let msg = db
        .insert_message(&conv.id, &user_sender(), MessageKind::Task, "t", None)
        .unwrap();
    let err = db.create_task(&msg.id, None, &bot.id, None, MAX_TASK_HOPS + 1, "");
    assert!(err.is_err());
}

#[test]
fn reply_budget_is_an_atomic_gate() {
    let (db, bot) = setup();
    let conv = db.dm_conversation(&bot.id).unwrap().unwrap();
    let msg = db
        .insert_message(&conv.id, &user_sender(), MessageKind::Task, "t", None)
        .unwrap();
    let task = db.create_task(&msg.id, None, &bot.id, None, 1, "").unwrap();
    for _ in 0..MAX_TASK_REPLIES {
        assert!(db.try_count_task_reply(&task.id, MAX_TASK_REPLIES).unwrap());
    }
    assert!(!db.try_count_task_reply(&task.id, MAX_TASK_REPLIES).unwrap());
    let t = db.get_task(&task.id).unwrap().unwrap();
    assert_eq!(t.reply_count, MAX_TASK_REPLIES);
}

#[test]
fn fanout_counts_open_children_per_chain_position() {
    let (db, bot) = setup();
    let other = db
        .create_bot(&bot.project_id, "bob", "", "", "", "/tmp/bob", "bob", None)
        .unwrap();
    let conv = db.dm_conversation(&other.id).unwrap().unwrap();
    let chain = bot.id.as_str();
    let mut tasks = Vec::new();
    for i in 0..2 {
        let msg = db
            .insert_message(&conv.id, &user_sender(), MessageKind::Task, "t", None)
            .unwrap();
        let t = db
            .create_task(&msg.id, Some(&bot.id), &other.id, None, i, chain)
            .unwrap();
        tasks.push(t);
    }
    let open = |chain| {
        db.open_tasks_delegated_by(&bot.id, Some(chain))
            .unwrap()
            .len()
    };
    assert_eq!(open(chain), 2);
    assert_eq!(open("elsewhere"), 0);
    assert_eq!(db.open_tasks_delegated_by(&bot.id, None).unwrap().len(), 2);
    db.set_task_state(&tasks[0].id, TaskState::Done).unwrap();
    assert_eq!(open(chain), 1);
}

#[test]
fn closing_a_task_succeeds_exactly_once() {
    let (db, bot) = setup();
    let conv = db.dm_conversation(&bot.id).unwrap().unwrap();
    let msg = db
        .insert_message(&conv.id, &user_sender(), MessageKind::Task, "t", None)
        .unwrap();
    let task = db.create_task(&msg.id, None, &bot.id, None, 1, "").unwrap();
    assert!(db.try_close_task(&task.id, TaskState::Cancelled).unwrap());
    // A second closer loses the race and must not overwrite the outcome.
    assert!(!db.try_close_task(&task.id, TaskState::Done).unwrap());
    let t = db.get_task(&task.id).unwrap().unwrap();
    assert_eq!(t.state, TaskState::Cancelled);
}

#[test]
fn expiring_a_task_succeeds_exactly_once() {
    let (db, bot) = setup();
    let conv = db.dm_conversation(&bot.id).unwrap().unwrap();
    let msg = db
        .insert_message(&conv.id, &user_sender(), MessageKind::Task, "t", None)
        .unwrap();
    let deadline = now() - chrono::Duration::hours(1);
    let task = db
        .create_task(&msg.id, None, &bot.id, Some(deadline), 1, "")
        .unwrap();
    let overdue = db.overdue_open_tasks(now()).unwrap();
    assert_eq!(overdue.len(), 1);
    assert!(db.expire_task(&task.id).unwrap());
    assert!(!db.expire_task(&task.id).unwrap());
    assert!(db.overdue_open_tasks(now()).unwrap().is_empty());
    let t = db.get_task(&task.id).unwrap().unwrap();
    assert_eq!(t.state, TaskState::Expired);
}

#[test]
fn a_task_without_a_deadline_never_expires() {
    let (db, bot) = setup();
    let conv = db.dm_conversation(&bot.id).unwrap().unwrap();
    let msg = db
        .insert_message(&conv.id, &user_sender(), MessageKind::Task, "t", None)
        .unwrap();
    db.create_task(&msg.id, None, &bot.id, None, 1, "").unwrap();
    assert!(db.overdue_open_tasks(now()).unwrap().is_empty());
}
