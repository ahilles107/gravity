//! The tag taxonomy over the control plane: the owner's half.

mod common;

use common::tasks::project_with_bots;
use common::WsClient;
use serde_json::json;

/// Raise a decision as a bot, filed under `spend`, and return its id.
async fn raised(clients: &mut [common::McpClient], who: usize, title: &str) -> String {
    let raised = clients[who]
        .call(
            "raise_decision",
            json!({"title": title, "body": "Spend is $40/day.", "tags": ["spend"]}),
        )
        .await;
    raised["decision"]["id"]
        .as_str()
        .expect("decision id")
        .to_string()
}

#[tokio::test]
async fn the_owner_renames_a_tag_in_place_and_can_delete_it_outright() {
    let (pair, mut bots) = project_with_bots(&["auction"]).await;
    let id = raised(&mut bots, 0, "Pause the campaign?").await;
    let mut owner = WsClient::connect(&pair.d).await;

    let described = owner
        .request(json!({
            "type": "upsert_tag", "name": "spend",
            "description": "money leaving", "color": "#f00"
        }))
        .await;
    assert_eq!(described["tag"]["name"], json!("spend"), "{described}");

    let renamed = owner
        .request(json!({"type": "rename_tag", "name": "spend", "to": "spending"}))
        .await;
    assert_eq!(renamed["tag"]["name"], json!("spending"), "{renamed}");
    assert_eq!(renamed["tag"]["description"], json!("money leaving"));

    let tags = owner.request(json!({"type": "list_tags"})).await;
    let tag = &tags["tags"][0];
    assert_eq!(tag["name"], json!("spending"), "{tags}");
    // One decision is filed under it and the owner still owes an answer.
    assert_eq!(tag["open_uses"], json!(1), "{tags}");
    assert!(tag["last_used_at"].is_string(), "{tags}");

    // The decision kept the tag, because it was never linked by name.
    let decision = owner
        .request(json!({"type": "get_decision", "decision_id": id}))
        .await;
    assert_eq!(decision["decision"]["tags"], json!(["spending"]));

    // Renaming onto a name in use would be a merge, which is retire_tag's job.
    owner
        .request(json!({"type": "upsert_tag", "name": "ads"}))
        .await;
    let refused = owner
        .request(json!({"type": "rename_tag", "name": "ads", "to": "spending"}))
        .await;
    assert_eq!(refused["code"], json!("conflict"), "{refused}");

    let deleted = owner
        .request(json!({"type": "delete_tag", "name": "spending"}))
        .await;
    assert_eq!(deleted["type"], json!("ok"), "{deleted}");
    let tags = owner.request(json!({"type": "list_tags"})).await;
    assert_eq!(tags["tags"][0]["name"], json!("ads"), "{tags}");
    assert_eq!(tags["tags"].as_array().unwrap().len(), 1, "{tags}");
    // The decision outlives the category it was filed under.
    let decision = owner
        .request(json!({"type": "get_decision", "decision_id": id}))
        .await;
    assert_eq!(decision["decision"]["tags"], json!([]), "{decision}");

    let gone = owner
        .request(json!({"type": "delete_tag", "name": "spending"}))
        .await;
    assert_eq!(gone["code"], json!("not_found"), "{gone}");
}
