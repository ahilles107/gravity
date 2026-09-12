//! The tag tools.
//!
//! Bots maintain the taxonomy themselves — with two exceptions. A tag carrying
//! settled history is not a bot's to retire: unfiling rulings is the owner's
//! call. Neither is a tag another project files under, because the taxonomy is
//! shared and a merge reaches every project at once. Both errors say so rather
//! than failing silently.

use bus::MAX_SETTLED_FOR_BOT_TAG_RETIRE;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::app::AppState;
use crate::decisions::{checked_color, checked_tag_description, checked_tags};

use super::caller;

fn tag_name(args: &Value) -> anyhow::Result<String> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("'name' is required"))?;
    let names = checked_tags(std::slice::from_ref(&name.to_string()))?;
    names
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("'name' is required"))
}

pub(super) fn list_tags(app: &Arc<AppState>, bot_id: &str) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let tags: Vec<Value> = app
        .db
        .list_tags_with_uses()?
        .into_iter()
        .filter(|t| t.tag.retired_at.is_none())
        .map(|t| {
            json!({
                "name": t.tag.name,
                "description": t.tag.description,
                // What this project files under it, not the global total: a
                // bot picking a tag cares what its own colleagues mean by it.
                "uses": t.uses.get(&me.project_id).copied().unwrap_or(0)
            })
        })
        .collect();
    Ok(json!({ "tags": tags }))
}

pub(super) fn upsert_tag(app: &Arc<AppState>, bot_id: &str, args: &Value) -> anyhow::Result<Value> {
    let _ = caller(app, bot_id)?;
    let name = tag_name(args)?;
    let tag = app.db.upsert_tag(
        &name,
        checked_tag_description(args.get("description").and_then(|v| v.as_str()))?,
        checked_color(args.get("color").and_then(|v| v.as_str()))?,
        bot_id,
    )?;
    Ok(json!({ "tag": tag }))
}

pub(super) fn retire_tag(app: &Arc<AppState>, bot_id: &str, args: &Value) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let name = tag_name(args)?;
    let tag = app
        .db
        .get_tag(&name)?
        .ok_or_else(|| anyhow::anyhow!("no tag named '{name}' — list_tags shows what exists"))?;
    let settled = app.db.settled_uses(&tag.id)?;
    if settled > MAX_SETTLED_FOR_BOT_TAG_RETIRE {
        anyhow::bail!(
            "'{name}' files {settled} settled decisions, more than the \
             {MAX_SETTLED_FOR_BOT_TAG_RETIRE} a bot may retire — unfiling the owner's rulings \
             is their call. Ask them in the Control center, or leave it in place."
        );
    }
    // The taxonomy is one table across every project: retiring hides this tag
    // from their pickers too, and `into` moves their filings outright.
    let others = app.db.tag_other_projects(&tag.id, &me.project_id)?;
    if !others.is_empty() {
        anyhow::bail!(
            "'{name}' is also used by {} other project(s), and the taxonomy is shared — \
             retiring it there is the owner's call. Ask them in the Control center.",
            others.len()
        );
    }
    let into = match args.get("into").and_then(|v| v.as_str()) {
        Some(into) => Some(checked_tags(std::slice::from_ref(&into.to_string()))?.remove(0)),
        None => None,
    };
    let retired = app.db.retire_tag(&name, into.as_deref())?;
    Ok(json!({ "tag": retired }))
}
