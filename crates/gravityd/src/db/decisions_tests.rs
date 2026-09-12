//! Persistence tests for the decision registry.

use super::tests::setup;
use super::*;

fn raise(db: &Db, bot: &Bot, title: &str) -> Decision {
    db.insert_decision(NewDecision {
        project_id: &bot.project_id,
        kind: DecisionKind::Decision,
        title,
        body: "context",
        options: &[],
        recommendation: None,
        raised_by_bot_id: &bot.id,
        on_behalf_of_bot_id: None,
        origin_chain: "",
        source_message_id: None,
        source_task_id: None,
        priority: Priority::Normal,
        deadline_at: None,
        supersedes_id: None,
        settled: None,
    })
    .unwrap()
}

fn ruling(text: &str) -> Ruling {
    Ruling {
        option: None,
        text: text.to_string(),
        reason: None,
        answered_at: now(),
        answered_by: "owner".to_string(),
    }
}

#[test]
fn a_decision_round_trips_through_the_row_mapper() {
    let (db, bot) = setup();
    let raised = raise(&db, &bot, "Pause the Apple Ads campaign?");
    let read = db.get_decision(&raised.id).unwrap().unwrap();
    assert_eq!(read.title, "Pause the Apple Ads campaign?");
    assert_eq!(read.state, DecisionState::Open);
    assert!(read.ruling.is_none());
}

#[test]
fn a_gate_refuses_a_transition_from_the_wrong_state() {
    let (db, bot) = setup();
    let d = raise(&db, &bot, "ship it?");
    // Publishing an unanswered decision would deliver a ruling with no words.
    assert!(!db.try_publish(&d.id).unwrap());
    assert!(db.try_answer(&d.id, &ruling("yes")).unwrap());
    assert!(db.try_publish(&d.id).unwrap());
    // And once settled, nothing moves it but a reopen.
    assert!(!db.try_withdraw(&d.id, "changed my mind").unwrap());
    assert!(!db.try_hold(&d.id, None).unwrap());
}

#[test]
fn discarding_a_draft_puts_the_decision_back_on_the_pending_list() {
    let (db, bot) = setup();
    let d = raise(&db, &bot, "bump forgejo?");
    db.try_answer(&d.id, &ruling("do it")).unwrap();
    assert!(db.try_unanswer(&d.id).unwrap());
    let read = db.get_decision(&d.id).unwrap().unwrap();
    assert_eq!(read.state, DecisionState::Open);
    assert!(read.ruling.is_none(), "the draft should be gone");
}

#[test]
fn one_bot_cannot_bury_the_inbox() {
    let (db, bot) = setup();
    for i in 0..MAX_OPEN_DECISIONS_PER_BOT {
        raise(&db, &bot, &format!("question {i}"));
    }
    let err = db
        .insert_decision(NewDecision {
            project_id: &bot.project_id,
            kind: DecisionKind::Decision,
            title: "one too many",
            body: "context",
            options: &[],
            recommendation: None,
            raised_by_bot_id: &bot.id,
            on_behalf_of_bot_id: None,
            origin_chain: "",
            source_message_id: None,
            source_task_id: None,
            priority: Priority::Normal,
            deadline_at: None,
            supersedes_id: None,
            settled: None,
        })
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(&MAX_OPEN_DECISIONS_PER_BOT.to_string()),
        "{err}"
    );
}

#[test]
fn the_same_question_asked_twice_is_recognised_whatever_the_punctuation() {
    let (db, bot) = setup();
    raise(&db, &bot, "Bump Forgejo to 16?");
    let found = db
        .open_duplicate(&bot.project_id, &bot.id, "  bump forgejo to 16  ")
        .unwrap();
    assert!(found.is_some(), "normalisation should match");
}

#[test]
fn a_settled_decision_is_no_longer_a_duplicate_to_worry_about() {
    let (db, bot) = setup();
    let d = raise(&db, &bot, "Bump Forgejo to 16?");
    db.try_answer(&d.id, &ruling("do it")).unwrap();
    db.try_publish(&d.id).unwrap();
    assert!(db
        .open_duplicate(&bot.project_id, &bot.id, "Bump Forgejo to 16?")
        .unwrap()
        .is_none());
}

#[test]
fn the_badge_counts_drafts_but_not_what_the_owner_parked() {
    let (db, bot) = setup();
    let open = raise(&db, &bot, "one");
    let drafted = raise(&db, &bot, "two");
    let held = raise(&db, &bot, "three");
    db.try_answer(&drafted.id, &ruling("yes")).unwrap();
    db.try_hold(&held.id, None).unwrap();
    let counts = db.count_pending_decisions(24).unwrap();
    assert_eq!(counts.total, 2, "open plus drafted, not held");
    assert_eq!(counts.by_project.get(&bot.project_id).copied(), Some(2));
    db.try_withdraw(&open.id, "no longer needed").unwrap();
    assert_eq!(db.count_pending_decisions(24).unwrap().total, 1);
}

#[test]
fn an_urgent_decision_and_a_near_deadline_are_counted_separately() {
    let (db, bot) = setup();
    let d = db
        .insert_decision(NewDecision {
            project_id: &bot.project_id,
            kind: DecisionKind::Decision,
            title: "ads",
            body: "context",
            options: &[],
            recommendation: None,
            raised_by_bot_id: &bot.id,
            on_behalf_of_bot_id: None,
            origin_chain: "",
            source_message_id: None,
            source_task_id: None,
            priority: Priority::Urgent,
            deadline_at: Some(now() + chrono::Duration::hours(3)),
            supersedes_id: None,
            settled: None,
        })
        .unwrap();
    let counts = db.count_pending_decisions(24).unwrap();
    assert_eq!(counts.urgent, 1);
    assert_eq!(counts.due_soon, 1);
    assert_eq!(db.decisions_due_within(24).unwrap().len(), 1);
    assert!(db.try_mark_deadline_notified(&d.id).unwrap());
    // Once, not every sweep: the failure this replaces was an item restated
    // twenty-seven times until the owner asked it to stop.
    assert!(!db.try_mark_deadline_notified(&d.id).unwrap());
    assert!(db.decisions_due_within(24).unwrap().is_empty());
}

#[test]
fn a_hold_that_has_run_out_comes_back() {
    let (db, bot) = setup();
    let d = raise(&db, &bot, "backblaze caps");
    db.try_hold(&d.id, Some(now() - chrono::Duration::hours(1)))
        .unwrap();
    let expired = db.held_expired().unwrap();
    assert_eq!(expired.len(), 1);
    assert!(db.try_resume(&d.id).unwrap());
    assert!(db.held_expired().unwrap().is_empty());
}

#[test]
fn publishing_twice_notifies_a_bot_once() {
    let (db, bot) = setup();
    let d = raise(&db, &bot, "ship it?");
    assert!(db.try_record_notification(&d.id, &bot.id, None).unwrap());
    assert!(!db.try_record_notification(&d.id, &bot.id, None).unwrap());
    assert_eq!(db.list_notifications(&d.id).unwrap().len(), 1);
    // An owner-requested re-notify is the one way the same ruling goes out
    // again, and it has to say so explicitly.
    db.clear_notifications(&d.id).unwrap();
    assert!(db.try_record_notification(&d.id, &bot.id, None).unwrap());
}

#[test]
fn a_relayed_ruling_can_only_be_confirmed_once() {
    let (db, bot) = setup();
    let d = db
        .insert_decision(NewDecision {
            project_id: &bot.project_id,
            kind: DecisionKind::Decision,
            title: "purge the R2 buckets",
            body: "asked at the terminal",
            options: &[],
            recommendation: None,
            raised_by_bot_id: &bot.id,
            on_behalf_of_bot_id: None,
            origin_chain: "",
            source_message_id: None,
            source_task_id: None,
            priority: Priority::Normal,
            deadline_at: None,
            supersedes_id: None,
            settled: Some(Ruling {
                option: None,
                text: "you have my permission to run the purge".to_string(),
                reason: None,
                answered_at: now(),
                answered_by: format!("owner-via-bot:{}", bot.id),
            }),
        })
        .unwrap();
    assert_eq!(d.state, DecisionState::Settled);
    assert!(db.try_confirm(&d.id).unwrap());
    assert!(!db.try_confirm(&d.id).unwrap(), "already the owner's own");
    let read = db.get_decision(&d.id).unwrap().unwrap();
    assert_eq!(read.ruling.unwrap().answered_by, "owner");
}

#[test]
fn the_cap_does_not_apply_to_a_ruling_already_given() {
    let (db, bot) = setup();
    for i in 0..MAX_OPEN_DECISIONS_PER_BOT {
        raise(&db, &bot, &format!("question {i}"));
    }
    // Backfilling a settled ledger must not be blocked by questions still open.
    db.insert_decision(NewDecision {
        project_id: &bot.project_id,
        kind: DecisionKind::Decision,
        title: "/boot is not backed up",
        body: "settled 1 Sep",
        options: &[],
        recommendation: None,
        raised_by_bot_id: &bot.id,
        on_behalf_of_bot_id: None,
        origin_chain: "",
        source_message_id: None,
        source_task_id: None,
        priority: Priority::Normal,
        deadline_at: None,
        supersedes_id: None,
        settled: Some(ruling("forget about it")),
    })
    .unwrap();
}

#[test]
fn full_text_search_follows_an_edit() {
    let (db, bot) = setup();
    let d = raise(&db, &bot, "quarterly attribution");
    let hits = db
        .list_decisions(&DecisionFilter {
            project_id: Some(&bot.project_id),
            query: Some("\"attribution\""),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    let mut edit = crate::decisions::current_edit(&d);
    edit.title = "monthly retention".to_string();
    db.update_decision(&d.id, &edit, "user").unwrap();
    let stale = db
        .list_decisions(&DecisionFilter {
            project_id: Some(&bot.project_id),
            query: Some("\"attribution\""),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert!(stale.is_empty(), "the update trigger should reindex");
    let fresh = db
        .list_decisions(&DecisionFilter {
            project_id: Some(&bot.project_id),
            query: Some("\"retention\""),
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(fresh.len(), 1);
}
