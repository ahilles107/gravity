//! Entity renderers shared by replies and pushes.
//!
//! A stored row is not what a client should see: an archived name is
//! tombstoned, and a bot's `state` and `unread_count` live outside the row
//! entirely. Every path that hands one of these to a client goes through here,
//! replies and pushes alike, so the two cannot drift.

use serde_json::{json, Value};

use crate::app::AppState;

/// A project as clients see it, with an archived row's original name restored.
pub(crate) fn project_view(project: &bus::Project) -> Value {
    json!({
        "id": project.id,
        "name": crate::db::Db::display_project_name(project),
        "dir_name": project.dir_name,
        "lead_bot_id": project.lead_bot_id,
        "deleted_at": project.deleted_at.map(|t| t.to_rfc3339()),
        "created_at": project.created_at.to_rfc3339()
    })
}

/// A bot as clients see it: the stored row plus the runtime fields the
/// supervisor and the delivery tables own.
pub(crate) fn bot_view(app: &AppState, bot: &bus::Bot) -> Value {
    let (state, reason) = app.supervisor.state(&bot.id);
    let unread = app.db.unread_count(&bot.id).unwrap_or(0);
    json!({
        "id": bot.id,
        "project_id": bot.project_id,
        // Archived rows carry a tombstoned name so the original is free to
        // reuse; clients should always see the name the bot actually had.
        "name": crate::db::Db::display_name(bot),
        "description": bot.description,
        "avatar": bot.avatar,
        "instructions": bot.instructions,
        "state": state.as_str(),
        "state_reason": reason,
        "unread_count": unread,
        "workspace_path": bot.workspace_path,
        "dir_name": bot.dir_name,
        "created_by_bot_id": bot.created_by_bot_id,
        "deleted_at": bot.deleted_at.map(|t| t.to_rfc3339()),
        "created_at": bot.created_at.to_rfc3339()
    })
}
