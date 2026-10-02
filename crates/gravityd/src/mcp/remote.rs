//! Bots creating and managing bots on another machine. `create_bot` with a
//! `machine` creates the bot on that peer, in the project linked with the
//! caller's; `update_bot` and `delete_bot` on a bot the caller created there
//! are forwarded to it. Each waits on the peer, so these run before the
//! synchronous tool table rather than inside it.

use std::sync::Arc;

use bus::Bot;
use serde_json::{json, Value};

use crate::app::AppState;
use crate::peer::remote_bots;

use super::caller;
use super::selfmgmt::{edit_from_args, my_child};

/// The tool's result when this call belongs on a peer, or `None` to handle it
/// here as usual.
pub(super) async fn intercept(
    app: &Arc<AppState>,
    bot_id: &str,
    name: &str,
    args: &Value,
) -> Option<anyhow::Result<Value>> {
    match name {
        "create_bot" => {
            let machine = args.get("machine").and_then(Value::as_str)?;
            Some(create(app, bot_id, machine, args).await)
        }
        "update_bot" | "delete_bot" => {
            let me = caller(app, bot_id).ok()?;
            let target = my_child(app, &me, args).ok()?;
            if !target.is_linked() {
                return None;
            }
            Some(if name == "update_bot" {
                update(app, &me, &target, args).await
            } else {
                delete(app, &me, &target, args).await
            })
        }
        _ => None,
    }
}

async fn create(
    app: &Arc<AppState>,
    bot_id: &str,
    machine: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let edit = edit_from_args(args, true)?;
    anyhow::ensure!(edit.name.is_some(), "'name' is required");
    let peer = app
        .db
        .get_peer_by_name(machine)?
        .filter(|p| p.revoked_at.is_none())
        .ok_or_else(|| anyhow::anyhow!("no machine named '{machine}'; list_bots shows them"))?;
    if app.db.project_link(&me.project_id, &peer.id)?.is_none() {
        anyhow::bail!(
            "this project is not linked with {}, so you cannot create a bot there; \
             create it here instead, or ask the owner to link the project",
            peer.name
        );
    }
    let bot = remote_bots::create(
        app,
        &me.project_id,
        &peer.id,
        remote_bots::fields(args, true),
        Some(&me),
    )
    .await?;
    Ok(json!({
        "name": bot.name,
        "description": bot.description,
        "runtime": bot.runtime,
        "machine": peer.name,
        "note": "Created and started on the other machine. Message it like any bot; \
                 you can update or delete it as one you created."
    }))
}

async fn update(
    app: &Arc<AppState>,
    me: &Bot,
    target: &Bot,
    args: &Value,
) -> anyhow::Result<Value> {
    // `name` addresses the target, as for a bot here.
    edit_from_args(args, false)?;
    let identity = remote_bots::fields(args, false);
    let updated = remote_bots::update(app, target, identity, Some(me)).await?;
    Ok(json!({
        "name": updated.name,
        "avatar": updated.avatar,
        "description": updated.description,
        "runtime": updated.runtime,
        "note": "Applied on the bot's machine."
    }))
}

async fn delete(
    app: &Arc<AppState>,
    me: &Bot,
    target: &Bot,
    args: &Value,
) -> anyhow::Result<Value> {
    let reason = args.get("reason").and_then(Value::as_str);
    remote_bots::delete(app, target, reason, Some(me)).await?;
    Ok(json!({
        "deleted": target.name,
        "note": "Deleted on its machine. Its history there is kept, but the bot \
                 cannot be restored."
    }))
}
