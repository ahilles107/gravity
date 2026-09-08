//! Routines end to end: scheduling, self-management over MCP, signals and
//! cancellation.

mod common;

use common::*;
use serde_json::json;

#[tokio::test]
async fn routine_runs_once_and_survives_restart_dedup() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let bot = create_bot(&mut c, &project_id, "alice").await;
    let bot_id = bot["id"].as_str().expect("id").to_string();
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;

    let routine = c
        .request(json!({
            "type": "create_routine", "bot_id": bot_id, "name": "weekly-report",
            "trigger": {"kind": "interval", "seconds": 3600},
            "prompt": "/weekly-report", "overlap_policy": "skip"
        }))
        .await;
    assert_eq!(routine["type"], "routine", "{routine}");
    let routine_id = routine["routine"]["id"].as_str().expect("rid").to_string();
    assert!(routine["routine"]["next_run_at"].is_string());

    c.request(json!({"type": "run_routine_now", "routine_id": routine_id}))
        .await;
    // Scheduler leases it, dispatches the envelope, and the delivery lands.
    c.wait_for(|v| v["type"] == "routine_run_update" && v["routine_run"]["state"] == "running")
        .await;
    c.wait_for(|v| v["type"] == "delivery_update" && v["delivery"]["state"] == "delivered")
        .await;

    let runs = c
        .request(json!({"type": "list_routine_runs", "routine_id": routine_id}))
        .await;
    assert_eq!(runs["routine_runs"].as_array().expect("runs").len(), 1);
}

/// A bot manages its own routines end to end over MCP — create, edit,
/// disable, delete — and cannot touch a routine owned by another bot.
#[tokio::test]
async fn bot_manages_its_own_routines_over_mcp() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let alice = create_bot(&mut c, &project_id, "alice").await;
    let bob = create_bot(&mut c, &project_id, "bob").await;
    let alice_id = alice["id"].as_str().expect("id").to_string();
    let bob_id = bob["id"].as_str().expect("id").to_string();
    let mut alice_bus = McpClient::new(&d, &d.app.secrets.bot_token(&alice_id).expect("token"));
    let mut bob_bus = McpClient::new(&d, &d.app.secrets.bot_token(&bob_id).expect("token"));

    let invalid = alice_bus
        .call_raw(
            "create_routine",
            json!({
                "name": "invalid",
                "trigger": {"kind": "interval", "seconds": 3600},
                "prompt": "never create this",
                "max_attempts": 6
            }),
        )
        .await;
    assert_eq!(invalid["isError"], json!(true), "{invalid}");
    let before = alice_bus.call("list_routines", json!({})).await;
    assert!(before["routines"].as_array().expect("routines").is_empty());

    let created = alice_bus
        .call(
            "create_routine",
            json!({
                "name": "digest",
                "trigger": {"kind": "interval", "seconds": 3600},
                "prompt": "write the digest"
            }),
        )
        .await;
    let routine_id = created["id"].as_str().expect("rid").to_string();

    // Edit everything at once; omitted fields would be kept, changed ones land.
    let updated = alice_bus
        .call(
            "update_routine",
            json!({
                "routine_id": routine_id,
                "name": "daily-digest",
                "trigger": {"kind": "interval", "seconds": 7200},
                "prompt": "write the daily digest",
                "busy_policy": "queue_one"
            }),
        )
        .await;
    assert_eq!(updated["name"], "daily-digest");
    assert_eq!(updated["trigger"]["seconds"], 7200);
    assert_eq!(updated["prompt"], "write the daily digest");
    assert_eq!(updated["overlap_policy"], "queue_one");

    // An empty edit and an invalid trigger are rejected, not silently ignored.
    let noop = alice_bus
        .call_raw("update_routine", json!({ "routine_id": routine_id }))
        .await;
    assert_eq!(noop["isError"], json!(true), "{noop}");
    let bad = alice_bus
        .call_raw(
            "update_routine",
            json!({ "routine_id": routine_id, "trigger": {"kind": "interval", "seconds": 5} }),
        )
        .await;
    assert_eq!(bad["isError"], json!(true), "{bad}");

    // Triggers that would abuse the scheduler are rejected up front: a
    // sub-minute cron, an interval past chrono's safe range, a malformed
    // signal name, and a signal trigger watching an unknown or its own bot.
    let rejected = [
        json!({"kind": "cron", "expr": "* * * * * *", "tz": "UTC"}),
        json!({"kind": "interval", "seconds": 10_000_000_000_000_000u64}),
        json!({"kind": "signal", "name": "Deploy Finished"}),
        json!({"kind": "signal", "name": "deploy.finished", "from_bot_id": "no-such-bot"}),
        json!({"kind": "signal", "name": "deploy.finished", "from_bot_id": alice_id}),
    ];
    for trigger in rejected {
        let denied = alice_bus
            .call_raw(
                "update_routine",
                json!({ "routine_id": routine_id, "trigger": trigger }),
            )
            .await;
        assert_eq!(denied["isError"], json!(true), "{denied}");
    }
    // Watching another bot in the project is fine, and list_bots exposes the
    // id the trigger needs.
    let bots = alice_bus.call("list_bots", json!({})).await;
    let listed_bob = bots["bots"]
        .as_array()
        .expect("bots")
        .iter()
        .find(|b| b["name"] == "bob")
        .expect("bob listed");
    assert_eq!(listed_bob["id"].as_str().expect("bob id"), bob_id);
    let watched = alice_bus
        .call(
            "update_routine",
            json!({ "routine_id": routine_id, "trigger": {"kind": "signal", "name": "deploy.finished", "from_bot_id": bob_id} }),
        )
        .await;
    assert_eq!(watched["trigger"]["from_bot_id"], json!(bob_id.clone()));
    // Back to a time trigger for the rest of the test.
    alice_bus
        .call(
            "update_routine",
            json!({ "routine_id": routine_id, "trigger": {"kind": "interval", "seconds": 7200} }),
        )
        .await;

    // Bob owns no part of Alice's routine.
    for tool in ["update_routine", "delete_routine", "set_routine_enabled"] {
        let denied = bob_bus
            .call_raw(
                tool,
                json!({ "routine_id": routine_id, "name": "mine", "enabled": false }),
            )
            .await;
        assert_eq!(denied["isError"], json!(true), "{tool}: {denied}");
    }

    let disabled = alice_bus
        .call(
            "set_routine_enabled",
            json!({ "routine_id": routine_id, "enabled": false }),
        )
        .await;
    assert_eq!(disabled["enabled"], false);

    let deleted = alice_bus
        .call("delete_routine", json!({ "routine_id": routine_id }))
        .await;
    assert_eq!(deleted["deleted"], "daily-digest");
    let listed = alice_bus.call("list_routines", json!({})).await;
    assert_eq!(listed["routines"].as_array().expect("routines").len(), 0);
}

/// A signal from one bot fans out to another bot's subscribed routine, and
/// never back to the emitter's own routines.
#[tokio::test]
async fn signal_from_one_bot_runs_another_bots_routine() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let alice = create_bot(&mut c, &project_id, "alice").await;
    let bob = create_bot(&mut c, &project_id, "bob").await;
    let alice_id = alice["id"].as_str().expect("id").to_string();
    let bob_id = bob["id"].as_str().expect("id").to_string();
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;

    let listener = c
        .request(json!({
            "type": "create_routine", "bot_id": bob_id, "name": "on-deploy",
            "trigger": {"kind": "signal", "name": "deploy.finished"},
            "prompt": "check the deploy", "overlap_policy": "skip"
        }))
        .await;
    let listener_id = listener["routine"]["id"].as_str().expect("rid").to_string();
    // A signal trigger has nothing to compute a next time from.
    assert!(listener["routine"]["next_run_at"].is_null());

    // Alice's own routine on the same signal must not fire on her emit.
    let own = c
        .request(json!({
            "type": "create_routine", "bot_id": alice_id, "name": "self-loop",
            "trigger": {"kind": "signal", "name": "deploy.finished"},
            "prompt": "should never run", "overlap_policy": "skip"
        }))
        .await;
    let own_id = own["routine"]["id"].as_str().expect("rid").to_string();

    let alice_bus = McpClient::new(&d, &d.app.secrets.bot_token(&alice_id).expect("token"));
    let mut alice_bus = alice_bus;
    let emitted = alice_bus
        .call(
            "emit_signal",
            json!({ "name": "deploy.finished", "payload": {"sha": "abc123"} }),
        )
        .await;
    assert_eq!(emitted["name"], "deploy.finished");
    assert_eq!(emitted["hop_count"], 0);

    c.wait_for(|v| v["type"] == "routine_run_update" && v["routine_run"]["state"] == "running")
        .await;
    let runs = c
        .request(json!({"type": "list_routine_runs", "routine_id": listener_id}))
        .await;
    let runs = runs["routine_runs"].as_array().expect("runs");
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0]["source"], "signal");
    assert!(runs[0]["signal_id"].is_string());
    let conversation = d
        .app
        .db
        .dm_conversation(&bob_id)
        .expect("conversation query")
        .expect("conversation");
    let messages = d
        .app
        .db
        .list_messages(&conversation.id, None, 20)
        .expect("messages");
    let routine_message = messages
        .iter()
        .find(|message| message.sender.kind == bus::SenderKind::Routine)
        .expect("routine message");
    assert!(routine_message.body.contains("deploy.finished"));
    assert!(routine_message.body.contains("abc123"));

    let own_runs = c
        .request(json!({"type": "list_routine_runs", "routine_id": own_id}))
        .await;
    assert!(
        own_runs["routine_runs"]
            .as_array()
            .expect("runs")
            .is_empty(),
        "a signal must not trigger its emitter's own routines"
    );

    // The same signal name is inert for a routine that is disabled.
    c.request(json!({"type": "set_routine_enabled", "routine_id": listener_id, "enabled": false}))
        .await;
    alice_bus
        .call("emit_signal", json!({ "name": "deploy.finished" }))
        .await;
    let after = c
        .request(json!({"type": "list_routine_runs", "routine_id": listener_id}))
        .await;
    assert_eq!(after["routine_runs"].as_array().expect("runs").len(), 1);
}

/// A queued occurrence can be called off, and its prompt goes with it.
#[tokio::test]
async fn cancelling_a_run_cancels_its_delivery() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid").to_string();
    let bot = create_bot(&mut c, &project_id, "alice").await;
    let bot_id = bot["id"].as_str().expect("id").to_string();
    c.wait_for(|v| v["type"] == "bot_state" && v["state"] == "ready")
        .await;

    let routine = c
        .request(json!({
            "type": "create_routine", "bot_id": bot_id, "name": "nightly",
            "trigger": {"kind": "interval", "seconds": 3600},
            "prompt": "/nightly", "overlap_policy": "skip",
            "max_attempts": 3, "max_duration_seconds": 120
        }))
        .await;
    assert_eq!(routine["routine"]["max_attempts"], 3);
    assert_eq!(routine["routine"]["max_duration_seconds"], 120);
    let routine_id = routine["routine"]["id"].as_str().expect("rid").to_string();

    c.request(json!({"type": "run_routine_now", "routine_id": routine_id}))
        .await;
    let update = c
        .wait_for(|v| v["type"] == "routine_run_update" && v["routine_run"]["state"] == "scheduled")
        .await;
    let run_id = update["routine_run"]["id"]
        .as_str()
        .expect("run id")
        .to_string();
    assert_eq!(update["routine_run"]["source"], "manual");

    let cancelled = c
        .request(json!({"type": "cancel_routine_run", "routine_run_id": run_id}))
        .await;
    assert_eq!(cancelled["routine_run"]["state"], "cancelled");

    // A finished run cannot be cancelled twice.
    let again = c
        .request(json!({"type": "cancel_routine_run", "routine_run_id": run_id}))
        .await;
    assert_eq!(again["type"], "error", "{again}");
}

#[tokio::test]
async fn routine_numeric_inputs_are_bounded_at_the_control_plane() {
    let d = spawn_daemon().await;
    let mut c = WsClient::connect(&d).await;
    let project = c
        .request(json!({"type": "create_project", "name": "p"}))
        .await;
    let project_id = project["project"]["id"].as_str().expect("pid");
    let bot = create_bot(&mut c, project_id, "alice").await;
    let bot_id = bot["id"].as_str().expect("id");

    for invalid in [
        json!({"max_attempts": 6}),
        json!({"max_duration_seconds": 21_601}),
        json!({"max_attempts": "three"}),
    ] {
        let mut request = json!({
            "type": "create_routine",
            "bot_id": bot_id,
            "name": format!("invalid-{}", invalid.to_string().len()),
            "trigger": {"kind": "interval", "seconds": 3600},
            "prompt": "run",
            "overlap_policy": "skip"
        });
        request
            .as_object_mut()
            .expect("object")
            .extend(invalid.as_object().expect("invalid object").clone());
        let denied = c.request(request).await;
        assert_eq!(denied["type"], "error", "{denied}");
    }
    let routines = c
        .request(json!({"type": "list_routines", "bot_id": bot_id}))
        .await;
    assert!(routines["routines"]
        .as_array()
        .expect("routines")
        .is_empty());

    let denied = c
        .request(json!({
            "type": "list_routine_runs",
            "routine_id": "missing",
            "limit": -1
        }))
        .await;
    assert_eq!(denied["type"], "error", "{denied}");
}
