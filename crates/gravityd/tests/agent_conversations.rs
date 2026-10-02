//! Conversations between bots: the pairs that have talked in a project, and
//! each pair's messages in order, read from the two bots' DMs together.

mod common;

use common::tasks::project_with_bots;
use common::*;
use serde_json::json;

#[tokio::test]
async fn pairs_and_their_threads_are_read_from_both_sides() {
    let (p, mut bots) = project_with_bots(&["lead", "dev", "qa"]).await;
    let (lead, dev, qa) = (p.ids[0].clone(), p.ids[1].clone(), p.ids[2].clone());
    let project_id =
        p.d.app
            .db
            .get_bot(&lead)
            .expect("db")
            .expect("bot")
            .project_id;

    bots[0]
        .call(
            "send_message",
            json!({"to": "dev", "kind": "task", "body": "build the login page"}),
        )
        .await;
    bots[1]
        .call(
            "send_message",
            json!({"to": "lead", "kind": "note", "body": "on it, starting with the form"}),
        )
        .await;
    bots[0]
        .call(
            "send_message",
            json!({"to": "qa", "kind": "note", "body": "heads up"}),
        )
        .await;

    let mut c = WsClient::connect(&p.d).await;
    let list = c
        .request(json!({"type": "list_agent_conversations", "project_id": project_id}))
        .await;
    assert_eq!(list["type"], "agent_conversations", "{list}");
    let conversations = list["conversations"].as_array().expect("conversations");
    assert_eq!(conversations.len(), 2, "{list}");
    // The most recent first: lead and qa talked last.
    let first: Vec<&str> = conversations[0]["bot_ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(first.contains(&lead.as_str()) && first.contains(&qa.as_str()));
    assert_eq!(conversations[1]["message_count"], 2);
    assert_eq!(
        conversations[1]["last"]["body"],
        "on it, starting with the form"
    );
    let names: Vec<&str> = list["bots"]
        .as_array()
        .expect("bots")
        .iter()
        .filter_map(|b| b["name"].as_str())
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");

    let thread = c
        .request(json!({
            "type": "list_agent_conversation", "project_id": project_id,
            "bot_ids": [dev, lead]
        }))
        .await;
    assert_eq!(thread["type"], "agent_conversation", "{thread}");
    let messages = thread["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["from_bot_id"], lead.as_str());
    assert_eq!(messages[0]["to_bot_id"], dev.as_str());
    assert_eq!(messages[0]["kind"], "task");
    assert_eq!(messages[0]["task"]["state"], "open");
    assert_eq!(messages[1]["from_bot_id"], dev.as_str());
    assert_eq!(thread["has_more"], false);
    assert_eq!(thread["bots"].as_array().expect("bots").len(), 2);

    // Paging back from the newest.
    let page = c
        .request(json!({
            "type": "list_agent_conversation", "project_id": project_id,
            "bot_ids": [lead, dev], "limit": 1
        }))
        .await;
    assert_eq!(page["messages"][0]["body"], "on it, starting with the form");
    assert_eq!(page["has_more"], true);
    let older = c
        .request(json!({
            "type": "list_agent_conversation", "project_id": project_id,
            "bot_ids": [lead, dev], "limit": 1, "before": page["messages"][0]["num"]
        }))
        .await;
    assert_eq!(older["messages"][0]["body"], "build the login page");
    assert_eq!(older["has_more"], false);

    let bad = c
        .request(json!({
            "type": "list_agent_conversation", "project_id": project_id, "bot_ids": [lead]
        }))
        .await;
    assert_eq!(bad["type"], "error");
}
