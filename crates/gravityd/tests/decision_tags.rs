//! Tags on decisions, as bots see them over MCP.

mod common;

use common::tasks::{error_text, project_with_bots, two_projects_one_bot_each};
use serde_json::json;

#[tokio::test]
async fn a_bot_may_not_retire_a_tag_that_files_the_owners_history() {
    let (pair, mut c) = project_with_bots(&["patch"]).await;
    c[0].call(
        "upsert_tag",
        json!({"name": "backups", "description": "what is and is not backed up"}),
    )
    .await;
    for i in 0..(bus::MAX_SETTLED_FOR_BOT_TAG_RETIRE + 1) {
        let raised = c[0]
            .call(
                "raise_decision",
                json!({"title": format!("backup question {i}"), "body": "context", "tags": ["backups"]}),
            )
            .await;
        let id = raised["decision"]["id"].as_str().unwrap().to_string();
        gravityd::decisions::answer(
            &pair.d.app,
            &gravityd::db::Actor::User,
            &id,
            None,
            "no",
            None,
        )
        .unwrap();
        gravityd::decisions::publish(&pair.d.app, &id, None).unwrap();
    }
    let raw = c[0]
        .call_raw("retire_tag", json!({"name": "backups"}))
        .await;
    let text = error_text(&raw);
    assert!(text.contains("their call"), "{text}");
}

#[tokio::test]
async fn tags_carry_their_description_so_two_bots_converge() {
    let (_pair, mut c) = project_with_bots(&["auction"]).await;
    c[0].call(
        "upsert_tag",
        json!({"name": "spend", "description": "money leaving the account"}),
    )
    .await;
    c[0].call(
        "raise_decision",
        json!({"title": "Ads?", "body": "x", "tags": ["Spend"]}),
    )
    .await;
    let tags = c[0].call("list_tags", json!({})).await;
    let listed = tags["tags"].as_array().unwrap();
    assert_eq!(listed.len(), 1, "case is not a new tag: {tags}");
    assert_eq!(listed[0]["description"], json!("money leaving the account"));
    assert_eq!(listed[0]["uses"], json!(1));
}

#[tokio::test]
async fn a_bot_cannot_retire_a_tag_another_project_files_under() {
    // The taxonomy is one table across every project, so a merge here moves
    // somebody else's filings.
    let (_pair, mut c) = two_projects_one_bot_each().await;
    c[1].call(
        "raise_decision",
        json!({"title": "Spend on ads?", "body": "x", "tags": ["spend"]}),
    )
    .await;
    c[0].call(
        "raise_decision",
        json!({"title": "Spend on disks?", "body": "x", "tags": ["spend"]}),
    )
    .await;

    let raw = c[0].call_raw("retire_tag", json!({"name": "spend"})).await;
    let text = error_text(&raw);
    assert!(text.contains("shared"), "{text}");
    let tags = c[1].call("list_tags", json!({})).await;
    assert_eq!(tags["tags"].as_array().unwrap().len(), 1, "{tags}");
}

#[tokio::test]
async fn a_bot_cannot_retag_a_decision_that_is_not_its_own() {
    let (pair, mut c) = project_with_bots(&["auction", "storefront"]).await;
    let raised = c[0]
        .call(
            "raise_decision",
            json!({"title": "Ads?", "body": "x", "tags": ["spend"]}),
        )
        .await;
    let id = raised["decision"]["id"].as_str().unwrap().to_string();

    let err = gravityd::decisions::set_tags(
        &pair.d.app,
        &gravityd::db::Actor::Bot {
            id: &pair.ids[1],
            project_id: &pair
                .d
                .app
                .db
                .get_bot(&pair.ids[1])
                .unwrap()
                .unwrap()
                .project_id,
        },
        &id,
        &["misc".to_string()],
    )
    .expect_err("a peer must not refile someone else's ask");
    assert!(err.to_string().contains("not yours to retag"), "{err}");

    // And the owner still can.
    gravityd::decisions::set_tags(
        &pair.d.app,
        &gravityd::db::Actor::User,
        &id,
        &["misc".to_string()],
    )
    .unwrap();
}

#[tokio::test]
async fn a_tag_colour_must_be_a_colour() {
    let (_pair, mut c) = project_with_bots(&["auction"]).await;
    let raw = c[0]
        .call_raw(
            "upsert_tag",
            json!({"name": "spend", "color": "red; background: url(x)"}),
        )
        .await;
    assert!(error_text(&raw).contains("not a colour"));
    c[0].call("upsert_tag", json!({"name": "spend", "color": "#7aa2f7"}))
        .await;
}
