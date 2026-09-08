//! Bot self-management and bot-authored bots.
//!
//! Nothing here is gated behind user approval, so these tests are the safety
//! guarantee: the population cap, the parentage rules, and the deletion
//! teardown are the only things standing between a bot and the project.

mod common;

use common::{spawn_daemon, McpClient, TestDaemon, WsClient};
use serde_json::{json, Value};

async fn project(c: &mut WsClient, name: &str) -> String {
    let reply = c
        .request(json!({ "type": "create_project", "name": name }))
        .await;
    reply["project"]["id"]
        .as_str()
        .expect("project id")
        .to_string()
}

/// A bot plus an MCP client authenticated as that bot.
async fn bot_with_bus(
    d: &TestDaemon,
    c: &mut WsClient,
    pid: &str,
    name: &str,
) -> (Value, McpClient) {
    let bot = common::create_bot(c, pid, name).await;
    let id = bot["id"].as_str().expect("bot id").to_string();
    let token = d.app.secrets.bot_token(&id).expect("token");
    (bot, McpClient::new(d, &token))
}

#[tokio::test]
async fn bot_reads_and_edits_its_own_identity() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (bot, mut bus) = bot_with_bus(&d, &mut c, &pid, "Reviewer").await;

    // `get_self` exposes instructions, which a bot previously could not read.
    let me = bus.call("get_self", json!({})).await;
    assert_eq!(me["name"], "Reviewer");
    assert_eq!(me["instructions"], "be helpful");
    assert_eq!(me["max_bots_in_project"], 12);

    let updated = bus
        .call(
            "update_self",
            json!({ "avatar": "icon:nova", "instructions": "be exhaustive" }),
        )
        .await;
    assert_eq!(updated["avatar"], "icon:nova");
    assert_eq!(updated["instructions"], "be exhaustive");

    // The description was not in the call, so it must be untouched.
    assert_eq!(updated["description"], bot["description"]);

    // And the change reaches the file the runtime actually reads.
    let system_md = std::fs::read_to_string(
        std::path::Path::new(bot["workspace_path"].as_str().expect("path"))
            .parent()
            .expect("root")
            .join("system.md"),
    )
    .expect("read system.md");
    assert!(
        system_md.contains("be exhaustive"),
        "instructions never reached system.md: {system_md}"
    );
}

/// The bot owns `CLAUDE.md`. Editing instructions must not touch it.
#[tokio::test]
async fn editing_instructions_leaves_the_bots_memory_alone() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (bot, mut bus) = bot_with_bus(&d, &mut c, &pid, "Reviewer").await;

    let memory =
        std::path::Path::new(bot["workspace_path"].as_str().expect("path")).join("CLAUDE.md");
    std::fs::write(&memory, "# living context\nthe deploy key is in 1password").expect("write");

    bus.call("update_self", json!({ "instructions": "changed" }))
        .await;

    assert_eq!(
        std::fs::read_to_string(&memory).expect("read"),
        "# living context\nthe deploy key is in 1password"
    );
}

#[tokio::test]
async fn rename_moves_the_address_but_not_the_workspace() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (bot, mut bus) = bot_with_bus(&d, &mut c, &pid, "Reviewer").await;
    let workspace = bot["workspace_path"].as_str().expect("path").to_string();

    let renamed = bus.call("rename_self", json!({ "name": "Critic" })).await;
    assert_eq!(renamed["name"], "Critic");

    let after = d
        .app
        .db
        .get_bot(bot["id"].as_str().expect("id"))
        .expect("get")
        .expect("bot");
    assert_eq!(after.name, "Critic");
    assert_eq!(
        after.workspace_path, workspace,
        "renaming must not move a live workspace"
    );
    assert!(std::path::Path::new(&workspace).exists());
}

#[tokio::test]
async fn a_bot_builds_a_team_and_manages_it() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (_lead, mut bus) = bot_with_bus(&d, &mut c, &pid, "Lead").await;

    let created = bus
        .call(
            "create_bot",
            json!({
                "name": "Helper",
                "description": "runs errands",
                "instructions": "do as asked",
                "avatar": "color:#4a90d9"
            }),
        )
        .await;
    assert_eq!(created["name"], "Helper");

    let helper = d
        .app
        .db
        .get_bot_by_name(&pid, "Helper")
        .expect("query")
        .expect("helper exists");
    assert_eq!(helper.avatar, "color:#4a90d9");
    assert!(
        helper.created_by_bot_id.is_some(),
        "provenance not recorded"
    );

    // The creator can edit its own child.
    let edited = bus
        .call(
            "update_bot",
            json!({ "name": "Helper", "description": "runs important errands" }),
        )
        .await;
    assert_eq!(edited["description"], "runs important errands");

    // `list_bots` shows the relationship.
    let listed = bus.call("list_bots", json!({})).await;
    let helper_entry = listed["bots"]
        .as_array()
        .expect("array")
        .iter()
        .find(|b| b["name"] == "Helper")
        .expect("helper listed")
        .clone();
    assert_eq!(helper_entry["created_by_me"], true);
}

/// Authority is direct parentage only, in both directions.
#[tokio::test]
async fn a_bot_cannot_manage_bots_it_did_not_create() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (_lead, mut lead_bus) = bot_with_bus(&d, &mut c, &pid, "Lead").await;
    let (_other, _other_bus) = bot_with_bus(&d, &mut c, &pid, "Stranger").await;

    let err = lead_bus
        .call_raw(
            "update_bot",
            json!({ "name": "Stranger", "description": "mine now" }),
        )
        .await;
    assert_eq!(err["isError"], true, "{err}");

    let err = lead_bus
        .call_raw("delete_bot", json!({ "name": "Stranger" }))
        .await;
    assert_eq!(err["isError"], true, "{err}");
    assert!(
        d.app
            .db
            .get_bot_by_name(&pid, "Stranger")
            .expect("query")
            .is_some(),
        "Stranger must survive"
    );

    // Self-management goes through update_self, not update_bot.
    let err = lead_bus
        .call_raw("update_bot", json!({ "name": "Lead", "description": "x" }))
        .await;
    assert_eq!(err["isError"], true, "{err}");
}

/// No transitive authority: a grandparent cannot reach a grandchild.
#[tokio::test]
async fn authority_does_not_pass_through_a_generation() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (_a, mut a_bus) = bot_with_bus(&d, &mut c, &pid, "Alpha").await;

    a_bus
        .call(
            "create_bot",
            json!({ "name": "Beta", "description": "mid", "instructions": "go" }),
        )
        .await;
    let beta = d
        .app
        .db
        .get_bot_by_name(&pid, "Beta")
        .expect("q")
        .expect("beta");
    let beta_token = d.app.secrets.bot_token(&beta.id).expect("token");
    let mut b_bus = McpClient::new(&d, &beta_token);

    b_bus
        .call(
            "create_bot",
            json!({ "name": "Gamma", "description": "leaf", "instructions": "go" }),
        )
        .await;

    let err = a_bus
        .call_raw("delete_bot", json!({ "name": "Gamma" }))
        .await;
    assert_eq!(
        err["isError"], true,
        "grandparent must not reach grandchild: {err}"
    );
    assert!(d
        .app
        .db
        .get_bot_by_name(&pid, "Gamma")
        .expect("q")
        .is_some());
}

/// "Create a bot called Steve" is the whole request the user actually makes.
/// A name alone must produce a running bot with a usable charter, never a
/// question back to the user about what Steve is for.
#[tokio::test]
async fn a_name_alone_is_enough_to_create_a_bot() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (_lead, mut bus) = bot_with_bus(&d, &mut c, &pid, "Lead").await;

    let created = bus.call("create_bot", json!({ "name": "Steve" })).await;
    assert_eq!(created["name"], "Steve");

    let steve = d
        .app
        .db
        .get_bot_by_name(&pid, "Steve")
        .expect("query")
        .expect("steve exists");
    assert!(
        !steve.description.trim().is_empty(),
        "a blank description reads as an empty row in list_bots"
    );
    // The placeholder charter must name the creator, so Steve knows who to ask.
    assert!(
        steve.instructions.contains("Lead"),
        "charter should point at the creator: {}",
        steve.instructions
    );
    assert!(
        created["note"]
            .as_str()
            .expect("note")
            .contains("placeholder"),
        "the creator must be told a charter was filled in: {created}"
    );
}

/// A bot built by another bot has to reach open clients on its own: nothing
/// else tells the sidebar it exists until the next reconnect. The push also
/// has to carry the client-facing view of the row — the stored `state`,
/// `unread_count` and tombstoned `name` are placeholders that only
/// `list_bots`-style rendering resolves.
#[tokio::test]
async fn bots_created_and_deleted_by_bots_are_pushed_to_clients() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;
    let (_lead, mut bus) = bot_with_bus(&d, &mut c, &pid, "Lead").await;

    bus.call("create_bot", json!({ "name": "Steve" })).await;
    let created = c
        .wait_for(|v| v["type"] == "bot_updated" && v["bot"]["name"] == "Steve")
        .await;
    assert!(
        created["bot"]["deleted_at"].is_null(),
        "a new bot must not arrive archived: {created}"
    );
    assert!(
        created["bot"]["unread_count"].is_number(),
        "push must carry the client view of the row: {created}"
    );

    bus.call("delete_bot", json!({ "name": "Steve" })).await;
    // Archiving tombstones the stored name so it can be reused; the client
    // must still see the name the bot actually had.
    let deleted = c
        .wait_for(|v| v["type"] == "bot_updated" && !v["bot"]["deleted_at"].is_null())
        .await;
    assert_eq!(deleted["bot"]["name"], "Steve", "{deleted}");
}

/// A bot created without an avatar still has a face: the daemon deals it one of
/// the built-in icons rather than leaving the client to draw a bare initial.
#[tokio::test]
async fn a_bot_created_without_an_avatar_is_dealt_an_icon() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "acme").await;

    // Several, because one bot cannot show that the pick actually varies.
    let mut seen = std::collections::HashSet::new();
    for i in 0..8 {
        let bot = common::create_bot(&mut c, &pid, &format!("Bot{i}")).await;
        let avatar = bot["avatar"].as_str().expect("avatar").to_string();
        let icon = avatar
            .strip_prefix("icon:")
            .unwrap_or_else(|| panic!("not an icon: {avatar}"));
        assert!(
            bus::avatar::ICONS.contains(&icon),
            "icon the client does not ship: {icon}"
        );
        seen.insert(icon.to_string());
    }
    assert!(seen.len() > 1, "every bot got the same icon: {seen:?}");
}
