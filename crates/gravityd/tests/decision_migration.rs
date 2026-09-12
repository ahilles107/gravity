//! Upgrading a database to the decision registry (migration 12).
//!
//! The registry is the one table set that is never retained away, and the one
//! index that would fail silently if its triggers were wrong. Both are checked
//! here against a database built by hand at an older schema version.

mod legacy;

use gravityd::db::{Db, DecisionFilter, NewDecision};
use legacy::v2_database;
use rusqlite::Connection;

fn raise(db: &Db, bot_id: &str, title: &str, source_message_id: Option<&str>) -> bus::Decision {
    db.insert_decision(NewDecision {
        project_id: "p1",
        kind: bus::DecisionKind::Decision,
        title,
        body: "Spend is $40/day against a listing that just went 17+.",
        options: &[],
        recommendation: None,
        raised_by_bot_id: bot_id,
        on_behalf_of_bot_id: None,
        origin_chain: "",
        source_message_id,
        source_task_id: None,
        priority: bus::Priority::Normal,
        deadline_at: None,
        supersedes_id: None,
        settled: None,
    })
    .expect("raise")
}

/// Migration 12 lands the decision registry on a populated database.
///
/// The FTS index is the part worth proving: an external-content table whose
/// triggers are wrong stays silently empty, and a registry nobody can search
/// is the markdown ledger again with extra steps.
#[test]
fn upgrading_lands_the_decision_registry_ready_to_search() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    let bot_id = v2_database(&path, "/tmp/acme/bots/reviewer/workspace");

    let db = Db::open(&path).expect("migrate");

    let decision = raise(&db, &bot_id, "Pause the Apple Ads campaign?", Some("m1"));

    let hits = db
        .list_decisions(&DecisionFilter {
            project_id: Some("p1"),
            query: Some("\"campaign\""),
            limit: 10,
            ..Default::default()
        })
        .expect("search");
    assert_eq!(hits.len(), 1, "the insert trigger must populate the index");
    assert_eq!(hits[0].id, decision.id);

    // A project starts with no lead; the fallback is the raising bot's creator.
    let project = db.get_project("p1").expect("q").expect("project");
    assert!(project.lead_bot_id.is_none());
    assert!(db.integrity_check().expect("integrity"));
}

/// Retention predates the registry and would happily delete the exchange a
/// decision cites, leaving a ruling nobody can re-read the reason for.
#[test]
fn retention_on_an_upgraded_database_spares_what_a_decision_cites() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    let bot_id = v2_database(&path, "/tmp/acme/bots/reviewer/workspace");
    let db = Db::open(&path).expect("migrate");
    raise(&db, &bot_id, "Pause?", Some("m1"));

    db.prune(1, 1, 1, 1).expect("prune");

    assert!(db.get_message("m1").expect("q").is_some());
    assert!(db.integrity_check().expect("integrity"));
}

/// Every foreign key the registry adds has to resolve on a real database.
#[test]
fn the_upgraded_database_has_no_dangling_references() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("bus.sqlite");
    let bot_id = v2_database(&path, "/tmp/acme/bots/reviewer/workspace");
    {
        let db = Db::open(&path).expect("migrate");
        let decision = raise(&db, &bot_id, "Ship it?", None);
        db.set_decision_tags(&decision.id, &["spend".to_string()], "user")
            .expect("tags");
        db.try_record_notification(&decision.id, &bot_id, None)
            .expect("notify");
        db.set_project_lead("p1", Some(&bot_id)).expect("lead");
    }

    let conn = Connection::open(&path).expect("reopen");
    let mut stmt = conn.prepare("PRAGMA foreign_key_check").expect("prepare");
    let violations: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .expect("check")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert!(violations.is_empty(), "dangling: {violations:?}");
}
