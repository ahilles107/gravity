//! Bot lifecycle shared by the control plane (`ws`) and the bus (`mcp`).
//!
//! Both entry points funnel through here so a bot editing itself and a user
//! editing that bot take exactly the same path: same validation, same audit
//! trail, same file regeneration. The only difference is the [`Actor`] recorded
//! and, for MCP, a parentage check applied before calling in.

use std::sync::Arc;

use bus::{Bot, MessageKind, RevisionField};

use crate::app::AppState;
use crate::db::Actor;
use crate::events::Push;
use crate::messaging::{self, daemon_sender, Dm};
use crate::paths::{self, BotProvision};
use crate::supervisor::BOT_TOKEN_ENV;

mod archive;
mod charter;
mod create;
mod revert;

pub use archive::{archive_bot, prune_archived_workspaces};
pub use create::{create_bot, Created};
pub use revert::revert_revision;

/// Fields that may change on a bot. `None` leaves the stored value alone,
/// which is what lets `update_self(avatar: "…")` avoid touching instructions.
#[derive(Default)]
pub struct IdentityEdit<'a> {
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub instructions: Option<&'a str>,
    pub avatar: Option<&'a str>,
}

impl IdentityEdit<'_> {
    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.instructions.is_none()
            && self.avatar.is_none()
    }
}

/// A bot already holds the requested name in this project.
///
/// Typed rather than a bare message so that a caller which picked the name
/// itself — one-click creation allocating `New Bot N` — can tell a lost race
/// apart from a name the user actually typed, and retry instead of reporting
/// a clash over a name that was never shown.
#[derive(Debug, thiserror::Error)]
#[error("a bot named '{0}' already exists in this project")]
pub struct NameTaken(pub String);

/// Validate a name for use in a project, rejecting duplicates.
///
/// `existing` is the bot being renamed, so it does not collide with itself.
pub fn validate_name(
    app: &Arc<AppState>,
    project_id: &str,
    raw: &str,
    existing: Option<&str>,
) -> anyhow::Result<String> {
    let name = bus::names::validate(raw).map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Some(clash) = app.db.get_bot_by_name(project_id, &name)? {
        if Some(clash.id.as_str()) != existing {
            return Err(NameTaken(name).into());
        }
    }
    Ok(name)
}
pub(super) fn provision_spec<'a>(
    app: &'a Arc<AppState>,
    project: &'a bus::Project,
    bot: &'a Bot,
) -> BotProvision<'a> {
    BotProvision {
        project_name: &project.name,
        project_dir_name: &project.dir_name,
        bot_id: &bot.id,
        name: &bot.name,
        dir_name: &bot.dir_name,
        description: &bot.description,
        instructions: &bot.instructions,
        daemon_port: app.cfg.port,
        bot_token_env: BOT_TOKEN_ENV,
        max_bots_per_project: app.cfg.max_bots_per_project,
        artifacts_dir: paths::artifacts_dir(&app.cfg, &project.dir_name)
            .display()
            .to_string(),
    }
}
pub(super) fn parse_avatar(raw: &str) -> anyhow::Result<String> {
    bus::avatar::parse(raw)
        .map(|a| a.as_stored())
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Apply an identity edit: validate, persist, record revisions, regenerate
/// `system.md`, and tell a running session what changed.
pub fn apply_identity_edit(
    app: &Arc<AppState>,
    bot: &Bot,
    edit: &IdentityEdit<'_>,
    actor: &Actor<'_>,
) -> anyhow::Result<Bot> {
    if edit.is_empty() {
        return Ok(bot.clone());
    }
    let name = match edit.name {
        Some(raw) => Some(validate_name(app, &bot.project_id, raw, Some(&bot.id))?),
        None => None,
    };
    let avatar = match edit.avatar {
        Some(raw) => Some(parse_avatar(raw)?),
        None => None,
    };

    // Only real changes are stored or recorded; a no-op write would otherwise
    // fill the audit trail with noise every time a bot re-sends its own state.
    let changes: Vec<(RevisionField, String, String)> = [
        (RevisionField::Name, name.clone(), bot.name.clone()),
        (
            RevisionField::Description,
            edit.description.map(str::to_string),
            bot.description.clone(),
        ),
        (
            RevisionField::Instructions,
            edit.instructions.map(str::to_string),
            bot.instructions.clone(),
        ),
        (RevisionField::Avatar, avatar.clone(), bot.avatar.clone()),
    ]
    .into_iter()
    .filter_map(|(field, new, old)| new.filter(|n| *n != old).map(|n| (field, old, n)))
    .collect();
    if changes.is_empty() {
        return Ok(bot.clone());
    }

    let changed = |f: RevisionField| changes.iter().any(|(field, _, _)| *field == f);
    app.db.update_bot(
        &bot.id,
        changed(RevisionField::Name)
            .then_some(())
            .and(name.as_deref()),
        edit.description
            .filter(|_| changed(RevisionField::Description)),
        edit.instructions
            .filter(|_| changed(RevisionField::Instructions)),
        changed(RevisionField::Avatar)
            .then_some(())
            .and(avatar.as_deref()),
    )?;
    for (field, old, new) in &changes {
        app.db.record_revision(&bot.id, actor, *field, old, new)?;
    }

    let updated = app
        .db
        .get_bot(&bot.id)?
        .ok_or_else(|| anyhow::anyhow!("bot vanished during update"))?;
    reprovision(app, &updated)?;

    if changed(RevisionField::Name) {
        announce_rename(app, bot, &updated)?;
    }
    let prompt_changed =
        changed(RevisionField::Description) || changed(RevisionField::Instructions);
    if prompt_changed {
        notify_running_session(app, &updated, actor)?;
    }
    app.events.push(Push::BotUpdated {
        bot: updated.clone(),
    });
    Ok(updated)
}

/// Rewrite the daemon-owned files for a bot after its identity changed.
pub fn reprovision(app: &Arc<AppState>, bot: &Bot) -> anyhow::Result<()> {
    let project = app
        .db
        .get_project(&bot.project_id)?
        .ok_or_else(|| anyhow::anyhow!("project not found"))?;
    let root = paths::bot_dir(&app.cfg, &project.dir_name, &bot.dir_name);
    paths::write_system_md(&root, &provision_spec(app, &project, bot))
}

/// Regenerate every bot's `system.md` from the database.
///
/// Run at startup so bots provisioned before instructions moved out of
/// `CLAUDE.md` pick up the new layout. The write is content-hash guarded, so
/// this is a no-op once each bot is current.
pub fn regenerate_all_system_md(app: &Arc<AppState>) -> anyhow::Result<()> {
    for bot in app.db.list_bots(None)? {
        if let Err(e) = reprovision(app, &bot) {
            tracing::warn!(bot_id = %bot.id, error = %e, "system.md regeneration failed");
        }
    }
    Ok(())
}

/// A rename changes the address other bots use in `send_message` and `@name`,
/// so the project is told. Bot-to-bot hygiene, not a user prompt.
fn announce_rename(app: &Arc<AppState>, before: &Bot, after: &Bot) -> anyhow::Result<()> {
    let peers = app.db.list_bots(Some(&after.project_id))?;
    for peer in peers.iter().filter(|p| p.id != after.id) {
        let body = format!(
            "Bot \"{}\" is now called \"{}\". Use the new name to reach it.",
            before.name, after.name
        );
        messaging::send_dm(
            &app.db,
            &app.events,
            Dm::new(&peer.id, &daemon_sender(), MessageKind::Note, &body),
        )?;
    }
    Ok(())
}

/// Instructions reach the runtime through `--append-system-prompt`, which is
/// read only at spawn — and restarting would drop the Claude Code session, so
/// an edit must not force one. Instead the file is already correct for the next
/// start, and the live session is told what changed as a bus message.
///
/// A bot that edited itself already knows, so it is not told twice.
fn notify_running_session(app: &Arc<AppState>, bot: &Bot, actor: &Actor<'_>) -> anyhow::Result<()> {
    if actor.bot_id() == Some(bot.id.as_str()) {
        return Ok(());
    }
    let (state, _) = app.supervisor.state(&bot.id);
    if !state.is_running() {
        return Ok(());
    }
    let body = format!(
        "Your configuration was updated.\n\nDescription: {}\n\nInstructions:\n{}",
        bot.description,
        if bot.instructions.trim().is_empty() {
            "(none)"
        } else {
            bot.instructions.trim()
        }
    );
    messaging::send_dm(
        &app.db,
        &app.events,
        Dm::new(&bot.id, &daemon_sender(), MessageKind::Note, &body),
    )?;
    Ok(())
}
