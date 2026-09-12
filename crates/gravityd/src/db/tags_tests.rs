//! Tag taxonomy and record deletion.

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

/// The taxonomy entry for one tag, with its usage rolled up.
fn usage(db: &Db, name: &str) -> TagUsage {
    db.list_tags_with_uses()
        .unwrap()
        .into_iter()
        .find(|u| u.tag.name == name)
        .expect("tag in the taxonomy")
}

#[test]
fn a_tag_is_the_same_tag_whatever_its_case() {
    let (db, bot) = setup();
    db.upsert_tag("Spend", Some("money leaving"), None, &bot.id)
        .unwrap();
    let again = db.upsert_tag("SPEND", None, Some("#f00"), &bot.id).unwrap();
    assert_eq!(again.name, "spend");
    // Upsert improves rather than clobbers: a bot adding a colour must not
    // erase the description someone else wrote.
    assert_eq!(again.description, "money leaving");
    assert_eq!(again.color, "#f00");
    assert_eq!(db.list_tags_with_uses().unwrap().len(), 1);
}

#[test]
fn merging_relinks_every_decision_and_keeps_the_history() {
    let (db, bot) = setup();
    let a = raise(&db, &bot, "one");
    let b = raise(&db, &bot, "two");
    db.set_decision_tags(&a.id, &["budget".to_string()], &bot.id)
        .unwrap();
    // `b` already carries both, which is what makes the merge a dedupe.
    db.set_decision_tags(&b.id, &["budget".to_string(), "spend".to_string()], &bot.id)
        .unwrap();
    db.retire_tag("budget", Some("spend")).unwrap();

    let tags = db.tags_for(&[a.id.clone(), b.id.clone()]).unwrap();
    assert_eq!(tags.get(&a.id).unwrap(), &vec!["spend".to_string()]);
    assert_eq!(tags.get(&b.id).unwrap(), &vec!["spend".to_string()]);
    let budget = db.get_tag("budget").unwrap().unwrap();
    assert!(budget.retired_at.is_some(), "retired, not deleted");
}

#[test]
fn retiring_without_a_target_leaves_the_links_in_place() {
    let (db, bot) = setup();
    let d = raise(&db, &bot, "one");
    db.set_decision_tags(&d.id, &["plex".to_string()], &bot.id)
        .unwrap();
    db.retire_tag("plex", None).unwrap();
    let tags = db.tags_for(std::slice::from_ref(&d.id)).unwrap();
    assert_eq!(tags.get(&d.id).unwrap(), &vec!["plex".to_string()]);
}

#[test]
fn a_tag_reports_how_much_settled_history_it_carries() {
    let (db, bot) = setup();
    let d = raise(&db, &bot, "one");
    db.set_decision_tags(&d.id, &["backups".to_string()], &bot.id)
        .unwrap();
    let tag = db.get_tag("backups").unwrap().unwrap();
    assert_eq!(db.settled_uses(&tag.id).unwrap(), 0);
    db.try_answer(
        &d.id,
        &Ruling {
            option: None,
            text: "forget about it".to_string(),
            reason: None,
            answered_at: now(),
            answered_by: "owner".to_string(),
        },
    )
    .unwrap();
    db.try_publish(&d.id).unwrap();
    assert_eq!(db.settled_uses(&tag.id).unwrap(), 1);
}

#[test]
fn setting_tags_replaces_rather_than_adds() {
    let (db, bot) = setup();
    let d = raise(&db, &bot, "one");
    db.set_decision_tags(&d.id, &["spend".to_string(), "ads".to_string()], &bot.id)
        .unwrap();
    db.set_decision_tags(&d.id, &["ads".to_string()], &bot.id)
        .unwrap();
    let tags = db.tags_for(std::slice::from_ref(&d.id)).unwrap();
    assert_eq!(tags.get(&d.id).unwrap(), &vec!["ads".to_string()]);
}

#[test]
fn deleting_a_decision_leaves_the_bot_its_history() {
    let (db, bot) = setup();
    let d = raise(&db, &bot, "one");
    db.set_decision_tags(&d.id, &["spend".to_string()], &bot.id)
        .unwrap();
    db.insert_decision_comment(&d.id, CommentAuthorKind::User, None, "tell me more")
        .unwrap();
    db.try_record_notification(&d.id, &bot.id, None).unwrap();
    let conv = db.dm_conversation(&bot.id).unwrap().unwrap();
    let msg = db
        .insert_message(
            &conv.id,
            &Sender {
                kind: SenderKind::User,
                bot_id: None,
                name: "user".to_string(),
            },
            MessageKind::Note,
            "[decision ...] forget about it",
            None,
            Some(&d.id),
        )
        .unwrap();

    assert!(db.delete_decision(&d.id).unwrap());
    assert!(db.get_decision(&d.id).unwrap().is_none());
    // The bot read that ruling and acted on it; the message stays, unlinked.
    let kept = db.get_message(&msg.id).unwrap().unwrap();
    assert!(kept.decision_id.is_none());
    assert!(db.list_decision_comments(&d.id).unwrap().is_empty());
}

#[test]
fn retention_never_prunes_the_exchange_a_decision_cites() {
    let (db, bot) = setup();
    let conv = db.dm_conversation(&bot.id).unwrap().unwrap();
    let msg = db
        .insert_message(
            &conv.id,
            &Sender {
                kind: SenderKind::User,
                bot_id: None,
                name: "user".to_string(),
            },
            MessageKind::Chat,
            "should we pause the campaign?",
            None,
            None,
        )
        .unwrap();
    db.insert_decision(NewDecision {
        project_id: &bot.project_id,
        kind: DecisionKind::Decision,
        title: "pause?",
        body: "context",
        options: &[],
        recommendation: None,
        raised_by_bot_id: &bot.id,
        on_behalf_of_bot_id: None,
        origin_chain: "",
        source_message_id: Some(&msg.id),
        source_task_id: None,
        priority: Priority::Normal,
        deadline_at: None,
        supersedes_id: None,
        settled: None,
    })
    .unwrap();
    // Backdate it past every retention window.
    db.lock()
        .execute(
            "UPDATE message SET created_at = '2000-01-01T00:00:00Z' WHERE id = ?1",
            rusqlite::params![msg.id],
        )
        .unwrap();
    // Without the guard the message goes, and the decision is left citing a
    // row that no longer exists — a ruling nobody can re-read the reason for.
    db.prune(1, 1, 1, 1).unwrap();
    assert!(db.get_message(&msg.id).unwrap().is_some());
}

#[test]
fn renaming_a_tag_renames_it_everywhere_it_is_filed() {
    let (db, bot) = setup();
    let d = raise(&db, &bot, "one");
    db.set_decision_tags(&d.id, &["spent".to_string()], &bot.id)
        .unwrap();
    let renamed = db.rename_tag("Spent", "spend").unwrap();
    assert_eq!(renamed.name, "spend");
    // The link is by id, so nothing had to be relinked for this to hold.
    let tags = db.tags_for(std::slice::from_ref(&d.id)).unwrap();
    assert_eq!(tags.get(&d.id).unwrap(), &vec!["spend".to_string()]);
}

#[test]
fn renaming_onto_a_name_in_use_leaves_both_tags_alone() {
    let (db, bot) = setup();
    let d = raise(&db, &bot, "one");
    db.set_decision_tags(&d.id, &["spend".to_string(), "ads".to_string()], &bot.id)
        .unwrap();
    assert!(db.rename_tag("ads", "spend").is_err(), "that is a merge");
    let tags = db.tags_for(std::slice::from_ref(&d.id)).unwrap();
    assert_eq!(
        tags.get(&d.id).unwrap(),
        &vec!["ads".to_string(), "spend".to_string()]
    );
}

#[test]
fn deleting_a_tag_unfiles_the_decision_without_losing_it() {
    let (db, bot) = setup();
    let d = raise(&db, &bot, "one");
    db.set_decision_tags(&d.id, &["plex".to_string()], &bot.id)
        .unwrap();
    assert!(db.delete_tag("plex").unwrap());
    assert!(db.get_tag("plex").unwrap().is_none());
    assert!(db.tags_for(std::slice::from_ref(&d.id)).unwrap().is_empty());
    assert!(
        db.get_decision(&d.id).unwrap().is_some(),
        "only the category went"
    );
    assert!(!db.delete_tag("plex").unwrap());
}

#[test]
fn a_tag_dates_itself_by_the_last_decision_filed_under_it() {
    let (db, bot) = setup();
    let old = raise(&db, &bot, "one");
    let recent = raise(&db, &bot, "two");
    for d in [&old, &recent] {
        db.set_decision_tags(&d.id, &["backups".to_string()], &bot.id)
            .unwrap();
    }
    assert_eq!(
        usage(&db, "backups").last_used_at,
        Some(db.get_decision(&recent.id).unwrap().unwrap().created_at)
    );
    // Settling `old` makes it the most recent thing that happened to the tag.
    db.try_answer(
        &old.id,
        &Ruling {
            option: None,
            text: "forget about it".to_string(),
            reason: None,
            answered_at: now(),
            answered_by: "owner".to_string(),
        },
    )
    .unwrap();
    db.try_publish(&old.id).unwrap();
    assert_eq!(
        usage(&db, "backups").last_used_at,
        db.get_decision(&old.id).unwrap().unwrap().published_at
    );
}

#[test]
fn open_uses_counts_what_the_tag_still_owes_the_owner() {
    let (db, bot) = setup();
    let pending = raise(&db, &bot, "one");
    let held = raise(&db, &bot, "two");
    let settled = raise(&db, &bot, "three");
    for d in [&pending, &held, &settled] {
        db.set_decision_tags(&d.id, &["spend".to_string()], &bot.id)
            .unwrap();
    }
    db.try_hold(&held.id, None).unwrap();
    db.try_answer(
        &settled.id,
        &Ruling {
            option: None,
            text: "let it fire".to_string(),
            reason: None,
            answered_at: now(),
            answered_by: "owner".to_string(),
        },
    )
    .unwrap();
    db.try_publish(&settled.id).unwrap();
    // Parked still counts: the owner chose later, not never.
    assert_eq!(usage(&db, "spend").open_uses, 2);
}
