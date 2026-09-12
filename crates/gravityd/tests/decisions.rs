//! The decision registry, as bots see it over MCP.

mod common;

use common::tasks::{
    drain_all, drain_until, error_text, project_with_bots, two_projects_one_bot_each,
};
use serde_json::json;

#[tokio::test]
async fn a_bot_raises_a_decision_and_the_lead_is_told() {
    // Auction asks; the Chief of Staff is the lead and must not find out from
    // a bot's memory file three days later.
    let (pair, mut c) = project_with_bots(&["chief", "auction"]).await;
    let (chief_id, auction) = (pair.ids[0].clone(), 1);
    pair.d
        .app
        .db
        .set_project_lead(
            &pair
                .d
                .app
                .db
                .get_bot(&chief_id)
                .unwrap()
                .unwrap()
                .project_id,
            Some(&chief_id),
        )
        .unwrap();

    let raised = c[auction]
        .call(
            "raise_decision",
            json!({
                "title": "Waive rule 3 and start the ads today?",
                "body": "The 17+ listing is live. Spend starts at $40/day.",
                "options": [
                    {"key": "start", "label": "Start today"},
                    {"key": "hold", "label": "Hold until the listing settles"}
                ],
                "recommendation": "start",
                "tags": ["spend", "apple-ads"],
                "priority": "urgent",
                "on_behalf_of": "chief"
            }),
        )
        .await;
    assert_eq!(raised["decision"]["state"], json!("open"));
    assert_eq!(raised["decision"]["priority"], json!("urgent"));

    let notes = drain_until(&mut c[0], "· raised").await;
    assert!(
        notes.iter().any(|m| m["body"]
            .as_str()
            .is_some_and(|b| b.contains("· raised") && b.contains("Waive rule 3"))),
        "the lead should hear about the raise: {notes:?}"
    );
    assert!(
        notes.iter().any(|m| m["body"]
            .as_str()
            .is_some_and(|b| b.contains("only the owner can"))),
        "and should be told it cannot answer: {notes:?}"
    );
}

#[tokio::test]
async fn a_published_ruling_arrives_from_the_user_not_from_a_peer() {
    let (pair, mut c) = project_with_bots(&["auction"]).await;
    let raised = c[0]
        .call(
            "raise_decision",
            json!({"title": "Pause the campaign?", "body": "Spend is $40/day."}),
        )
        .await;
    let id = raised["decision"]["id"].as_str().unwrap().to_string();

    gravityd::decisions::answer(
        &pair.d.app,
        &gravityd::db::Actor::User,
        &id,
        None,
        "Let it fire, we learn more from the pause than from the spend.",
        None,
    )
    .unwrap();
    let outcome = gravityd::decisions::publish(&pair.d.app, &id, None).unwrap();
    assert_eq!(outcome.notified.len(), 1);

    let delivered = drain_until(&mut c[0], "· settled").await;
    let ruling = delivered
        .into_iter()
        .find(|m| m["body"].as_str().is_some_and(|b| b.contains("· settled")))
        .expect("no ruling delivered");
    let body = ruling["body"].as_str().unwrap();
    assert!(body.contains("from USER"), "{body}");
    assert!(body.contains("Let it fire"), "{body}");
    assert!(body.contains("not a relay"), "{body}");
    // No `[msg #N …]` wrapper: the decision header is the authority.
    assert!(body.starts_with("[decision "), "{body}");
}

#[tokio::test]
async fn publishing_twice_does_not_tell_a_bot_the_same_ruling_twice() {
    let (pair, mut c) = project_with_bots(&["auction"]).await;
    let raised = c[0]
        .call(
            "raise_decision",
            json!({"title": "Ship it?", "body": "RC is green."}),
        )
        .await;
    let id = raised["decision"]["id"].as_str().unwrap().to_string();
    gravityd::decisions::answer(
        &pair.d.app,
        &gravityd::db::Actor::User,
        &id,
        None,
        "GO",
        None,
    )
    .unwrap();
    gravityd::decisions::publish(&pair.d.app, &id, None).unwrap();
    gravityd::decisions::publish(&pair.d.app, &id, None).unwrap();

    let delivered = drain_all(&mut c[0]).await;
    let rulings = delivered
        .iter()
        .filter(|m| m["body"].as_str().is_some_and(|b| b.contains("· settled")))
        .count();
    assert_eq!(rulings, 1, "{delivered:?}");
}

#[tokio::test]
async fn a_bot_cannot_rule_on_its_own_decision() {
    let (_pair, mut c) = project_with_bots(&["auction"]).await;
    let raised = c[0]
        .call(
            "raise_decision",
            json!({"title": "Ship it?", "body": "RC is green."}),
        )
        .await;
    let id = raised["decision"]["id"].as_str().unwrap().to_string();
    // There is no answer tool at all — the refusal is that the tool a bot
    // reaches for does not exist, and the thread is where it adds context.
    let tools = c[0].tools().await;
    let names: Vec<&str> = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(!names.contains(&"answer_decision"), "{names:?}");
    assert!(names.contains(&"comment_decision"), "{names:?}");
    c[0].call(
        "comment_decision",
        json!({"id": id, "body": "Staging is green too."}),
    )
    .await;
}

#[tokio::test]
async fn check_inbox_reminds_a_bot_what_it_is_waiting_on() {
    let (_pair, mut c) = project_with_bots(&["auction"]).await;
    c[0].call(
        "raise_decision",
        json!({"title": "Waive rule 3?", "body": "Listing is live.", "deadline_at": "2026-09-13T00:00:00Z"}),
    )
    .await;
    let inbox = c[0].call("check_inbox", json!({})).await;
    let open = inbox["open_decisions"].as_array().expect("open_decisions");
    assert_eq!(open.len(), 1, "{inbox}");
    assert_eq!(open[0]["title"], json!("Waive rule 3?"));
    assert_eq!(open[0]["deadline_at"], json!("2026-09-13T00:00:00+00:00"));
}

#[tokio::test]
async fn the_same_question_twice_is_refused_with_the_id_of_the_first() {
    let (_pair, mut c) = project_with_bots(&["auction"]).await;
    let first = c[0]
        .call(
            "raise_decision",
            json!({"title": "Bump Forgejo to 16?", "body": "16 is out."}),
        )
        .await;
    let id = first["decision"]["id"].as_str().unwrap();
    let raw = c[0]
        .call_raw(
            "raise_decision",
            json!({"title": "bump forgejo to 16", "body": "16 is still out."}),
        )
        .await;
    let text = error_text(&raw);
    assert!(text.contains(id), "{text}");
    assert!(text.contains("comment_decision"), "{text}");

    // Naming what it supersedes is how a bot says "I know, the facts changed".
    c[0].call(
        "raise_decision",
        json!({"title": "bump forgejo to 16", "body": "16.1 fixes the blocker.", "supersedes": id}),
    )
    .await;
}

#[tokio::test]
async fn a_settled_ruling_comes_back_from_the_registry_rather_than_a_ledger() {
    let (pair, mut c) = project_with_bots(&["patch"]).await;
    let raised = c[0]
        .call(
            "raise_decision",
            json!({"title": "Pin Dozzle?", "body": "It updates often.", "tags": ["updates"]}),
        )
        .await;
    let id = raised["decision"]["id"].as_str().unwrap().to_string();
    gravityd::decisions::answer(
        &pair.d.app,
        &gravityd::db::Actor::User,
        &id,
        None,
        "It stays unpinned.",
        None,
    )
    .unwrap();
    gravityd::decisions::publish(&pair.d.app, &id, None).unwrap();

    let settled = c[0]
        .call(
            "list_decisions",
            json!({"state": "settled", "tags": ["updates"]}),
        )
        .await;
    let items = settled["decisions"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{settled}");
    assert_eq!(items[0]["ruling"]["text"], json!("It stays unpinned."));
    assert_eq!(items[0]["ruling"]["answered_by"], json!("owner"));
}

#[tokio::test]
async fn withdrawing_is_only_for_the_bot_that_asked() {
    let (_pair, mut c) = project_with_bots(&["auction", "storefront"]).await;
    let raised = c[0]
        .call("raise_decision", json!({"title": "Ads?", "body": "x"}))
        .await;
    let id = raised["decision"]["id"].as_str().unwrap().to_string();
    let raw = c[1]
        .call_raw(
            "withdraw_decision",
            json!({"id": id, "reason": "not needed"}),
        )
        .await;
    assert!(error_text(&raw).contains("not yours to withdraw"));
    c[0].call(
        "withdraw_decision",
        json!({"id": id, "reason": "answered itself"}),
    )
    .await;
}

#[tokio::test]
async fn a_bot_cannot_supersede_a_ruling_in_another_project() {
    // `supersedes` was the one bot-supplied field that wrote to another row:
    // it stamps `superseded_by_id`, so unchecked it let a bot mark a ruling it
    // cannot even read as no longer current.
    let (pair, mut c) = two_projects_one_bot_each().await;
    let theirs = c[1]
        .call(
            "raise_decision",
            json!({"title": "Move the backups off Tower?", "body": "context"}),
        )
        .await;
    let theirs_id = theirs["decision"]["id"].as_str().unwrap().to_string();

    let raw = c[0]
        .call_raw(
            "raise_decision",
            json!({"title": "Backups", "body": "x", "supersedes": theirs_id}),
        )
        .await;
    assert!(
        error_text(&raw).contains("another project"),
        "{}",
        error_text(&raw)
    );
    assert!(
        pair.d
            .app
            .db
            .get_decision(&theirs_id)
            .unwrap()
            .unwrap()
            .superseded_by_id
            .is_none(),
        "their ruling must be untouched"
    );

    // A decision that does not exist at all is a refusal too, not a dangling id.
    let raw = c[0]
        .call_raw(
            "raise_decision",
            json!({"title": "Backups", "body": "x", "supersedes": "nope"}),
        )
        .await;
    assert!(error_text(&raw).contains("no decision with id"));
}

#[tokio::test]
async fn a_plain_question_searches_rather_than_raising_an_fts_error() {
    // FTS5 parses the bound value as query syntax, so a bot writing the
    // sentence the tool description invites used to get `internal` back.
    let (_pair, mut c) = project_with_bots(&["auction"]).await;
    c[0].call(
        "raise_decision",
        json!({"title": "Should we use postgres?", "body": "sqlite is creaking"}),
    )
    .await;
    for query in [
        "should we use \"postgres",
        "cost:high",
        "a AND",
        "what about (this",
        "postgres",
    ] {
        let found = c[0]
            .call("list_decisions", json!({"state": "all", "query": query}))
            .await;
        assert!(
            found["decisions"].is_array(),
            "query {query:?} should search, not fail: {found}"
        );
    }
    let hit = c[0]
        .call(
            "list_decisions",
            json!({"state": "all", "query": "should we use postgres?"}),
        )
        .await;
    assert_eq!(hit["decisions"].as_array().unwrap().len(), 1, "{hit}");
}
