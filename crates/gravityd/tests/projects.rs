//! Renaming and deleting projects.
//!
//! Both mutations reach past the database: a rename must leave the filesystem
//! alone, and a delete must take every bot in the project down with it. Each
//! has a named test here because getting either wrong is silent — a moved
//! directory looks fine until a bot restarts, and a skipped bot keeps running
//! with a valid credential for a project the user believes is gone.

mod common;

use common::{spawn_daemon, TestDaemon, WsClient};
use serde_json::{json, Value};

async fn project(c: &mut WsClient, name: &str) -> Value {
    let reply = c
        .request(json!({ "type": "create_project", "name": name }))
        .await;
    assert_eq!(reply["type"], "project", "{reply}");
    reply["project"].clone()
}

fn id(value: &Value) -> &str {
    value["id"].as_str().expect("id")
}

fn project_names(d: &TestDaemon) -> Vec<String> {
    d.app
        .db
        .list_projects()
        .expect("list")
        .into_iter()
        .map(|p| p.name)
        .collect()
}

#[tokio::test]
async fn renaming_a_project_leaves_its_directory_alone() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let created = project(&mut c, "Acme").await;
    let bot = common::create_bot(&mut c, id(&created), "Reviewer").await;
    let workspace = bot["workspace_path"]
        .as_str()
        .expect("workspace")
        .to_string();

    let renamed = c
        .request(json!({
            "type": "update_project", "project_id": id(&created), "name": "Initech"
        }))
        .await;
    assert_eq!(renamed["type"], "project", "{renamed}");
    assert_eq!(renamed["project"]["name"], "Initech");
    assert_eq!(
        renamed["project"]["dir_name"], "acme",
        "the directory must not follow the name"
    );

    assert!(
        std::path::Path::new(&workspace).exists(),
        "the bot's workspace moved out from under it: {workspace}"
    );
    let stored = d
        .app
        .db
        .get_bot(bot["id"].as_str().expect("bot id"))
        .expect("get")
        .expect("bot");
    assert_eq!(stored.workspace_path, workspace);
}

/// The manifest on disk is what a user reads when the daemon is not running,
/// so it must not keep claiming the old name.
#[tokio::test]
async fn renaming_a_project_refreshes_the_files_that_name_it() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let created = project(&mut c, "Acme").await;
    common::create_bot(&mut c, id(&created), "Reviewer").await;

    c.request(json!({
        "type": "update_project", "project_id": id(&created), "name": "Initech"
    }))
    .await;

    let root = d.app.cfg.projects_dir().join("acme");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("project.json")).expect("read"))
            .expect("json");
    assert_eq!(manifest["name"], "Initech");

    let bot_file: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("bots/reviewer/bot.json")).expect("read"),
    )
    .expect("json");
    assert_eq!(bot_file["project"], "Initech");
    assert_eq!(bot_file["name"], "Reviewer", "the bot was not renamed");
}

#[tokio::test]
async fn a_project_cannot_be_renamed_onto_another() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let acme = project(&mut c, "Acme").await;
    project(&mut c, "Initech").await;

    let reply = c
        .request(json!({
            "type": "update_project", "project_id": id(&acme), "name": "Initech"
        }))
        .await;
    assert_eq!(reply["type"], "error", "{reply}");
    assert_eq!(reply["code"], "invalid_request");
    assert_eq!(project_names(&d), vec!["Acme", "Initech"]);
}

#[tokio::test]
async fn deleting_a_project_archives_every_bot_in_it() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let created = project(&mut c, "Acme").await;
    let lead = common::create_bot(&mut c, id(&created), "Lead").await;
    let helper = common::create_bot(&mut c, id(&created), "Helper").await;
    let tokens: Vec<(String, String)> = [&lead, &helper]
        .iter()
        .map(|bot| {
            let bot_id = bot["id"].as_str().expect("bot id").to_string();
            let token = d.app.secrets.bot_token(&bot_id).expect("token");
            (bot_id, token)
        })
        .collect();

    let reply = c
        .request(json!({ "type": "delete_project", "project_id": id(&created) }))
        .await;
    assert_eq!(reply["type"], "ok", "{reply}");

    assert!(
        project_names(&d).is_empty(),
        "archived project still listed"
    );
    for (bot_id, token) in &tokens {
        assert!(
            d.app.db.get_live_bot(bot_id).expect("get").is_none(),
            "bot {bot_id} survived its project"
        );
        assert!(
            d.app.secrets.bot_for_token(token).is_none(),
            "bot {bot_id} kept a working credential"
        );
    }
}

/// The name is tombstoned rather than held, so the obvious recovery from a
/// mistaken delete — create it again — works.
#[tokio::test]
async fn a_deleted_project_frees_its_name() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let created = project(&mut c, "Acme").await;

    c.request(json!({ "type": "delete_project", "project_id": id(&created) }))
        .await;
    let again = project(&mut c, "Acme").await;

    assert_ne!(id(&again), id(&created), "a new row, not the archived one");
    assert_eq!(
        again["dir_name"], "acme-2",
        "the archived directory is kept, so its name stays reserved"
    );
    assert_eq!(project_names(&d), vec!["Acme"]);
}

#[tokio::test]
async fn deleting_a_project_twice_is_reported_not_repeated() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let created = project(&mut c, "Acme").await;

    c.request(json!({ "type": "delete_project", "project_id": id(&created) }))
        .await;
    let reply = c
        .request(json!({ "type": "delete_project", "project_id": id(&created) }))
        .await;
    assert_eq!(reply["type"], "error", "{reply}");
    assert_eq!(reply["code"], "not_found");
}
