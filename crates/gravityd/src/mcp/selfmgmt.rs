//! Self-management and bot-authored bots.
//!
//! Every tool here applies immediately — nothing is proposed, queued, or held
//! for approval. The safety model is a population cap, the fact that a bot may
//! only touch itself and its own children, and a full audit trail with revert.
//!
//! Identity comes from the bearer token that authenticated the MCP request, so
//! there is no "which bot am I" argument for a bot to get wrong or forge.

use std::sync::Arc;

use bus::Bot;
use serde_json::{json, Value};

use crate::app::AppState;
use crate::botmgmt::{self, IdentityEdit};
use crate::db::Actor;

use super::caller;

/// Size limits on free-text identity fields. Generous for real use and small
/// enough that a runaway bot cannot write a novel into its own prompt.
const MAX_DESCRIPTION_BYTES: usize = 2 * 1024;
const MAX_INSTRUCTIONS_BYTES: usize = 32 * 1024;

fn checked_text(args: &Value, key: &str, max: usize) -> anyhow::Result<Option<String>> {
    let Some(value) = args.get(key).and_then(|v| v.as_str()) else {
        return Ok(None);
    };
    if value.len() > max {
        anyhow::bail!("'{key}' is {} bytes; the limit is {max}", value.len());
    }
    Ok(Some(value.to_string()))
}

fn edit_from_args(args: &Value, with_name: bool) -> anyhow::Result<IdentityEdit<'_>> {
    // Validated here, borrowed below: the checks need owned Strings for the
    // length test but IdentityEdit borrows, so re-read the (now known good)
    // fields from the original Value.
    checked_text(args, "description", MAX_DESCRIPTION_BYTES)?;
    checked_text(args, "instructions", MAX_INSTRUCTIONS_BYTES)?;
    Ok(IdentityEdit {
        name: with_name
            .then(|| args.get("name").and_then(|v| v.as_str()))
            .flatten(),
        description: args.get("description").and_then(|v| v.as_str()),
        instructions: args.get("instructions").and_then(|v| v.as_str()),
        avatar: args.get("avatar").and_then(|v| v.as_str()),
    })
}

/// Everything a bot knows about itself, including its own instructions — which
/// it previously had no way to read back.
pub(super) fn get_self(app: &Arc<AppState>, bot_id: &str) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let (state, reason) = app.supervisor.state(&me.id);
    let project = app.db.get_project(&me.project_id)?;
    let creator = match &me.created_by_bot_id {
        Some(id) => app.db.get_bot(id)?.map(|b| b.name),
        None => None,
    };
    let children: Vec<String> = app
        .db
        .children_of(&me.id)?
        .into_iter()
        .map(|b| b.name)
        .collect();
    Ok(json!({
        "name": me.name,
        "avatar": me.avatar,
        "description": me.description,
        "instructions": me.instructions,
        "project": project.map(|p| p.name),
        "workspace_path": me.workspace_path,
        "status": state.as_str(),
        "status_reason": reason,
        "created_by": creator,
        "bots_i_created": children,
        "bots_in_project": app.db.count_live_bots(&me.project_id)?,
        "max_bots_in_project": app.cfg.max_bots_per_project
    }))
}

pub(super) fn update_self(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let edit = edit_from_args(args, false)?;
    let updated = botmgmt::apply_identity_edit(app, &me, &edit, &Actor::Bot(bot_id))?;
    Ok(json!({
        "name": updated.name,
        "avatar": updated.avatar,
        "description": updated.description,
        "instructions": updated.instructions,
        "note": "Applied. Your system prompt is updated for your next session; \
                 you can act on the new instructions right away."
    }))
}

pub(super) fn rename_self(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("'name' is required"))?;
    let edit = IdentityEdit {
        name: Some(name),
        ..Default::default()
    };
    let updated = botmgmt::apply_identity_edit(app, &me, &edit, &Actor::Bot(bot_id))?;
    Ok(json!({
        "name": updated.name,
        "note": "Other bots in the project have been told your new name."
    }))
}

/// Create a bot in the caller's project. The new bot is the caller's child,
/// which is what later authorises `update_bot` and `delete_bot` on it.
pub(super) fn create_bot(app: &Arc<AppState>, bot_id: &str, args: &Value) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let edit = edit_from_args(args, true)?;
    if edit.name.is_none() {
        anyhow::bail!("'name' is required");
    }
    let created = botmgmt::create_bot(app, &me.project_id, &edit, Some(&me), &Actor::Bot(bot_id))?;
    // Bots are always-on, so there is no "created but not started" case.
    let mut note = "Created and started. You can send it a message now.".to_string();
    // Say when a charter was filled in, so the creator knows the new bot is
    // running on a placeholder it can replace with `update_bot`.
    let blank = |v: Option<&str>| v.map(str::trim).unwrap_or_default().is_empty();
    if blank(edit.description) || blank(edit.instructions) {
        note.push_str(
            " It had no description or instructions of its own, so it was given a \
             placeholder charter telling it to ask you what it is for; use \
             update_bot once you know.",
        );
    }
    Ok(json!({
        "name": created.bot.name,
        "description": created.bot.description,
        "instructions": created.bot.instructions,
        "note": note
    }))
}

/// Resolve a target bot the caller is allowed to manage.
///
/// Authority is direct parentage only — if A created B and B created C, A
/// cannot touch C. Provenance grants no transitive rights, so a deep team
/// cannot be reorganised from the top by surprise.
fn my_child(app: &Arc<AppState>, me: &Bot, args: &Value) -> anyhow::Result<Bot> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("'name' is required"))?;
    let target = app
        .db
        .get_bot_by_name(&me.project_id, name)?
        .ok_or_else(|| anyhow::anyhow!("no bot named '{name}' in this project"))?;
    if target.id == me.id {
        anyhow::bail!("use update_self or rename_self to change your own settings");
    }
    if target.created_by_bot_id.as_deref() != Some(me.id.as_str()) {
        anyhow::bail!("'{name}' was not created by you; you can only manage bots you created");
    }
    Ok(target)
}

pub(super) fn update_bot(app: &Arc<AppState>, bot_id: &str, args: &Value) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let target = my_child(app, &me, args)?;
    // `name` addresses the target here rather than renaming it; renaming a
    // child would silently change an address its own children may be using.
    let edit = edit_from_args(args, false)?;
    let updated = botmgmt::apply_identity_edit(app, &target, &edit, &Actor::Bot(bot_id))?;
    Ok(json!({
        "name": updated.name,
        "avatar": updated.avatar,
        "description": updated.description,
        "note": "Applied. The bot has been told its configuration changed."
    }))
}

/// Archive a bot the caller created.
///
/// Deletion is irreversible from the bot's side, so the tool result says so
/// plainly rather than implying the bot can be restored.
pub(super) fn delete_bot(app: &Arc<AppState>, bot_id: &str, args: &Value) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let target = my_child(app, &me, args)?;
    let reason = args.get("reason").and_then(|v| v.as_str());
    botmgmt::archive_bot(app, &target, &Actor::Bot(bot_id), reason)?;
    Ok(json!({
        "deleted": target.name,
        "bots_in_project": app.db.count_live_bots(&me.project_id)?,
        "note": "Deleted. Its workspace and message history are kept, but the bot \
                 cannot be restored. Anyone waiting on a task it held has been told."
    }))
}
