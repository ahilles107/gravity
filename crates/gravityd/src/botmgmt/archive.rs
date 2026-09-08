//! Archival deletion of a bot.
//!
//! The previous implementation deleted a handful of rows and left the rest
//! behind. That was survivable while only a human ever pressed Delete; it is
//! not now that bots delete each other. Four things it got wrong, all fixed
//! here:
//!
//! 1. It never removed the `bot` row, so a "deleted" bot stayed listed.
//! 2. It never revoked the bot's token, leaving `/mcp` and `/hook` open to it
//!    forever.
//! 3. It dropped open tasks silently, stranding every bot that had delegated
//!    work — they waited on a `complete_task` that could never arrive.
//! 4. It left the DM conversation and messages dangling.
//!
//! The workspace is deliberately *kept*. Deleting a bot should never destroy
//! work it produced; retention reclaims the directory later.

use std::sync::Arc;

use bus::{Bot, MessageKind, RevisionField, TaskState};

use crate::app::AppState;
use crate::db::Actor;
use crate::events::Push;
use crate::messaging::{self, daemon_sender};

/// Archive a bot: stop it, revoke its credential, release everyone waiting on
/// it, and mark the row deleted.
///
/// Callers are responsible for the authorisation decision — the MCP path allows
/// only direct children, the control plane allows anything.
pub fn archive_bot(
    app: &Arc<AppState>,
    bot: &Bot,
    actor: &Actor<'_>,
    reason: Option<&str>,
) -> anyhow::Result<()> {
    // Stop first: the runtime must not outlive its credential, or it would keep
    // making authenticated calls that then fail confusingly.
    app.supervisor.stop_bot(&bot.id)?;

    release_open_tasks(app, bot)?;

    // Without this the token stays valid in memory and on disk.
    app.secrets.remove_bot_token(&bot.id)?;

    app.db.archive_bot(&bot.id, &actor.as_stored())?;
    app.db.record_revision(
        &bot.id,
        actor,
        RevisionField::Deleted,
        &bot.name,
        reason.unwrap_or("deleted"),
    )?;

    app.events.push(Push::Notify {
        level: "info".to_string(),
        title: "Bot deleted".to_string(),
        body: match reason {
            Some(r) => format!("{} was deleted: {r}", bot.name),
            None => format!("{} was deleted.", bot.name),
        },
    });
    if let Some(archived) = app.db.get_bot(&bot.id)? {
        app.events.push(Push::BotUpdated { bot: archived });
    }
    tracing::info!(bot_id = %bot.id, name = %bot.name, by = %actor.as_stored(), "bot archived");
    Ok(())
}

/// Cancel every open task assigned to the bot and tell the requester.
///
/// A delegating bot blocks on a `done` message. Deleting the assignee without
/// this leaves it waiting forever, with no error and no timeout — the failure
/// mode is a silent hang, which is why the notice is a real bus message rather
/// than a log line.
fn release_open_tasks(app: &Arc<AppState>, bot: &Bot) -> anyhow::Result<()> {
    for task in app.db.open_tasks_for(&bot.id)? {
        app.db.set_task_state(&task.id, TaskState::Cancelled)?;

        let Some(requester_id) = task.from_bot_id.as_deref() else {
            // The user asked directly; the message already sits in a
            // conversation they can see, so no bus notice is needed.
            continue;
        };
        if app.db.get_live_bot(requester_id)?.is_none() {
            continue;
        }
        let body = format!(
            "Task cancelled: {} was deleted before finishing it. \
             Reassign the work if it still needs doing.",
            bot.name
        );
        messaging::send_dm(
            &app.db,
            &app.events,
            requester_id,
            &daemon_sender(),
            MessageKind::Done,
            &body,
            Some(&task.origin_message_id),
        )?;
    }
    Ok(())
}

/// Delete the workspaces of bots archived longer ago than the retention window.
///
/// Returns how many directories were removed. Failures are logged and skipped
/// rather than aborting the sweep: one unreadable directory should not stop the
/// rest being reclaimed.
pub fn prune_archived_workspaces(app: &Arc<AppState>, days: i64) -> anyhow::Result<usize> {
    let cutoff = chrono::Utc::now() - chrono::Duration::days(days.max(1));
    let mut removed = 0;
    for bot in app.db.list_bots_with_archived(None)? {
        let Some(deleted_at) = bot.deleted_at else {
            continue;
        };
        if deleted_at > cutoff {
            continue;
        }
        let workspace = std::path::Path::new(&bot.workspace_path);
        let Some(root) = workspace.parent() else {
            continue;
        };
        if !root.exists() {
            continue;
        }
        match std::fs::remove_dir_all(root) {
            Ok(()) => {
                removed += 1;
                tracing::info!(bot_id = %bot.id, path = %root.display(), "archived workspace reclaimed");
            }
            Err(e) => {
                tracing::warn!(bot_id = %bot.id, error = %e, "reclaiming workspace failed");
            }
        }
    }
    Ok(removed)
}
