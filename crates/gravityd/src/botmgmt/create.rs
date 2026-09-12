//! Creating a bot: name and workspace allocation, provisioning, first turn.

use std::sync::Arc;

use anyhow::bail;
use bus::{Bot, MessageKind, RevisionField};
use rand::seq::SliceRandom;

use crate::app::AppState;
use crate::config::RuntimeKind;
use crate::db::Actor;
use crate::events::Push;
use crate::messaging::{self, daemon_sender, Dm};
use crate::paths;

use super::{charter, parse_avatar, provision_spec, validate_name, IdentityEdit};

/// Outcome of creating a bot, including whether it actually started.
pub struct Created {
    pub bot: Bot,
}

///
/// Sanitizing is lossy — `"Bot A"` and `"Bot-A"` both yield `bot-a` — so a
/// plain derivation would eventually hand two bots the same workspace. Rather
/// than failing a creation over an internal detail, disambiguate with a
/// counter; the directory is never an address, so its exact spelling does not
/// matter as long as it is unique.
fn unique_dir_name(app: &Arc<AppState>, project_id: &str, name: &str) -> anyhow::Result<String> {
    let base = bus::names::dir_name(name);
    if !app.db.dir_name_taken(project_id, &base)? {
        return Ok(base);
    }
    for n in 2..1000 {
        let candidate = format!("{base}-{n}");
        if !app.db.dir_name_taken(project_id, &candidate)? {
            return Ok(candidate);
        }
    }
    bail!("cannot allocate a workspace directory for '{name}'")
}

/// Create a bot, provision its files, and start it.
///
/// `creator` is set when a bot builds another bot; it grants that bot edit and
/// delete rights over the result and shows up as provenance in the client.
pub fn create_bot(
    app: &Arc<AppState>,
    project_id: &str,
    edit: &IdentityEdit<'_>,
    creator: Option<&Bot>,
    actor: &Actor<'_>,
) -> anyhow::Result<Created> {
    let raw_name = edit.name.unwrap_or_default();
    let name = validate_name(app, project_id, raw_name, None)?;
    // A caller that says nothing about the avatar gets one anyway: "create a
    // bot called Steve" should produce a bot with a face, not a bare initial.
    let avatar = match edit.avatar.map(str::trim).filter(|a| !a.is_empty()) {
        Some(raw) => parse_avatar(raw)?,
        None => random_icon(),
    };

    // The population cap is the only limit on creation, so it is checked here
    // for every caller rather than at the MCP boundary.
    let live = app.db.count_live_bots(project_id)?;
    let cap = app.cfg.max_bots_per_project as i64;
    if live >= cap {
        bail!(
            "project is at its limit of {cap} bots ({live} live). \
             Delete a bot you no longer need to free a slot."
        );
    }

    // Asked before the insert below, and counting deleted bots too: someone
    // who cleared a project out has already met a bot and does not need the
    // product tour a second time.
    let first_in_project = !app.db.project_has_had_bots(project_id)?;

    let project = app
        .db
        .get_project(project_id)?
        .ok_or_else(|| anyhow::anyhow!("project not found"))?;
    let dir_name = unique_dir_name(app, project_id, &name)?;
    let workspace = paths::bot_dir(&app.cfg, &project.dir_name, &dir_name).join("workspace");

    // A name is the only thing a caller must supply; the rest is filled in so
    // that "create a bot called Steve" produces a working bot instead of a
    // question back to the user.
    let description = charter::description(edit.description);
    let instructions = charter::instructions(edit.instructions, creator.map(|c| c.name.as_str()));

    let bot = app.db.create_bot(
        project_id,
        &name,
        &description,
        &instructions,
        &avatar,
        &workspace.display().to_string(),
        &dir_name,
        creator.map(|c| c.id.as_str()),
    )?;

    // Issuing the token before provisioning means the runtime can authenticate
    // as soon as the files land.
    app.secrets.bot_token(&bot.id)?;
    let dirs = paths::provision_bot(&app.cfg, &provision_spec(app, &project, &bot))?;
    if app.cfg.runtime == RuntimeKind::Pty {
        if let Err(error) = paths::trust_workspace(&app.cfg.user_home, &dirs.workspace) {
            tracing::warn!(bot_id = %bot.id, %error, "could not trust new bot workspace");
        }
    }

    let origin = match creator {
        Some(c) => format!("created by {}", c.name),
        None => "created".to_string(),
    };
    app.db
        .record_revision(&bot.id, actor, RevisionField::Created, "", &origin)?;

    // Nothing gates creation, so this notice is how the user finds out. It is
    // a toast, not a prompt: ignorable, and the audit trail keeps the record.
    if let Some(c) = creator {
        app.events.push(Push::notice(
            "info",
            "Bot created",
            format!("{} created a new bot, {name}.", c.name),
        ));
    }

    // Bots are always-on, so a new one runs immediately: a bot that builds a
    // helper can message it in the same turn, and a user never has to start
    // anything. A failure here is not fatal — the supervision loop retries.
    if let Err(e) = app.supervisor.start_bot(&bot.id) {
        tracing::warn!(bot_id = %bot.id, error = %e, "created bot did not start; will retry");
    }

    // Opening turn, so the new bot's window is not blank until someone types
    // at it. Queued rather than delivered here: the session may not be up yet,
    // and the delivery worker already defers until it is. A failure only costs
    // the greeting, so it does not fail the creation.
    let charted = [edit.description, edit.instructions]
        .iter()
        .any(|field| field.is_some_and(|text| !text.trim().is_empty()));
    let greeting = charter::introduction(&charter::Introduction {
        creator: creator.map(|c| c.name.as_str()),
        charted,
        first: first_in_project,
    });
    if let Err(e) = messaging::send_dm(
        &app.db,
        &app.events,
        Dm::new(&bot.id, &daemon_sender(), MessageKind::Note, &greeting),
    ) {
        tracing::warn!(bot_id = %bot.id, error = %e, "introduction prompt not queued");
    }

    app.events.push(Push::BotUpdated { bot: bot.clone() });
    Ok(Created { bot })
}

/// The stored avatar for a bot created without one: a built-in icon, dealt at
/// random so a project's bots are visually distinct without anyone choosing.
///
/// Random rather than derived from the bot id: an id-derived pick would look
/// arbitrary in exactly the same way while making two bots collide for good,
/// with no way to reroll short of editing the avatar by hand.
fn random_icon() -> String {
    let icon = bus::avatar::ICONS
        .choose(&mut rand::thread_rng())
        .copied()
        .unwrap_or(bus::avatar::ICONS[0]);
    bus::avatar::Avatar::Icon(icon.to_string()).as_stored()
}
