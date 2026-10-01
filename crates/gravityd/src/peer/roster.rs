//! A linked project's roster: the bots it shows its peer, and the stand-ins
//! kept in step with what the peer shows back. See "Linked projects" in
//! `docs/peer-bots.md`.

use std::sync::{Arc, Mutex, MutexGuard};

use bus::{Bot, Peer, RemoteBot};

use crate::app::AppState;
use crate::botmgmt;
use crate::db::{Actor, Db};
use crate::events::Push;

/// Serialises every change to stand-ins. A roster event, the reply to a
/// `create_bot` and an unlink can land at once on different threads, and each
/// reads before it writes.
static STAND_INS: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    STAND_INS.lock().unwrap_or_else(|e| e.into_inner())
}

/// A project's bots that run here, as its peer stands them in.
pub(super) fn roster(app: &AppState, project_id: &str) -> anyhow::Result<Vec<RemoteBot>> {
    let project = app
        .db
        .get_project(project_id)?
        .map(|p| Db::display_project_name(&p))
        .unwrap_or_default();
    Ok(app
        .db
        .list_bots(Some(project_id))?
        .iter()
        .filter(|bot| !bot.is_linked())
        .map(|bot| super::inbound::remote_view(bot, project.clone()))
        .collect())
}

/// Lets the peer deliver to each of these bots.
pub(super) fn expose(app: &AppState, peer_id: &str, bots: &[RemoteBot]) -> anyhow::Result<()> {
    for bot in bots {
        app.db.expose_bot_to_peer(peer_id, &bot.id)?;
    }
    Ok(())
}

/// The names in `incoming` a bot in this project already holds, other than
/// the stand-in for that same bot.
pub(super) fn clashes(
    app: &AppState,
    project_id: &str,
    peer_id: &str,
    incoming: &[RemoteBot],
) -> anyhow::Result<Vec<String>> {
    let mut clashing = Vec::new();
    for remote in incoming {
        let Some(bot) = app.db.get_bot_by_name(project_id, &remote.name)? else {
            continue;
        };
        let same = bot.peer_id.as_deref() == Some(peer_id)
            && bot.remote_bot_id.as_deref() == Some(remote.id.as_str());
        if !same {
            clashing.push(remote.name.clone());
        }
    }
    Ok(clashing)
}

/// Bots address each other by name, so a clash would leave one of the two
/// unreachable. The owner renames one and links again.
pub(super) fn clash_refusal(names: &[String]) -> anyhow::Error {
    super::refuse(
        "conflict",
        format!(
            "bot names clash: {} exist on both machines; rename or delete one of each, \
             then link again",
            names.join(", ")
        ),
    )
}

/// Brings a linked project's stand-ins in line with the peer's roster: new
/// bots stood in, changed ones updated, missing ones archived.
pub(super) fn reconcile(
    app: &Arc<AppState>,
    peer: &Peer,
    project_id: &str,
    roster: &[RemoteBot],
) -> anyhow::Result<()> {
    let _guard = lock();
    for remote in roster {
        if let Err(e) = stand_in_locked(app, peer, project_id, remote) {
            tracing::warn!(peer = %peer.name, bot = %remote.name, error = %e,
                "could not mirror a linked bot");
        }
    }
    for bot in app.db.stand_ins(project_id, &peer.id)? {
        let listed = roster
            .iter()
            .any(|remote| bot.remote_bot_id.as_deref() == Some(remote.id.as_str()));
        if !listed {
            botmgmt::archive_bot(app, &bot, &Actor::User, Some("archived on its machine"))?;
        }
    }
    Ok(())
}

/// The stand-in for one of the peer's bots, created or brought up to date.
pub(super) fn stand_in(
    app: &Arc<AppState>,
    peer: &Peer,
    project_id: &str,
    remote: &RemoteBot,
) -> anyhow::Result<Bot> {
    let _guard = lock();
    stand_in_locked(app, peer, project_id, remote)
}

/// Archives a stand-in, unless something else already did.
pub(super) fn archive_stand_in(
    app: &Arc<AppState>,
    bot_id: &str,
    reason: &str,
) -> anyhow::Result<()> {
    let _guard = lock();
    if let Some(bot) = app.db.get_live_bot(bot_id)?.filter(Bot::is_linked) {
        botmgmt::archive_bot(app, &bot, &Actor::User, Some(reason))?;
    }
    Ok(())
}

fn stand_in_locked(
    app: &Arc<AppState>,
    peer: &Peer,
    project_id: &str,
    remote: &RemoteBot,
) -> anyhow::Result<Bot> {
    let Some(bot) = app.db.linked_bot(&peer.id, &remote.id, project_id)? else {
        let name = free_name(app, peer, project_id, &remote.name, None)?;
        let stand_in = RemoteBot {
            name,
            ..remote.clone()
        };
        let bot = app.db.create_linked_bot(project_id, &stand_in, &peer.id)?;
        tracing::info!(peer = %peer.name, bot = %bot.name, "linked bot mirrored");
        app.events.push(Push::BotUpdated { bot: bot.clone() });
        return Ok(bot);
    };
    let name = free_name(app, peer, project_id, &remote.name, Some(&bot.id))?;
    let identity =
        name != bot.name || bot.description != remote.description || bot.avatar != remote.avatar;
    if identity {
        app.db.update_bot(
            &bot.id,
            Some(&name),
            Some(&remote.description),
            None,
            Some(&remote.avatar),
        )?;
    }
    let runtime = bot.runtime != remote.runtime;
    if runtime {
        app.db.set_bot_runtime(&bot.id, remote.runtime)?;
    }
    if !identity && !runtime {
        return Ok(bot);
    }
    let updated = app
        .db
        .get_bot(&bot.id)?
        .ok_or_else(|| anyhow::anyhow!("linked bot vanished"))?;
    app.events.push(Push::BotUpdated {
        bot: updated.clone(),
    });
    Ok(updated)
}

/// The peer's name for its bot, or `<name>-<peer>` when a bot here holds it.
/// Links refuse clashing names, so this only matters when a bot created or
/// renamed later takes a name the other side already uses.
fn free_name(
    app: &AppState,
    peer: &Peer,
    project_id: &str,
    wanted: &str,
    holder: Option<&str>,
) -> anyhow::Result<String> {
    let name = bus::names::validate(wanted).map_err(anyhow::Error::msg)?;
    if !app.db.bot_name_taken_except(project_id, &name, holder)? {
        return Ok(name);
    }
    let fallback =
        bus::names::validate(&format!("{name}-{}", peer.name)).map_err(anyhow::Error::msg)?;
    anyhow::ensure!(
        !app.db
            .bot_name_taken_except(project_id, &fallback, holder)?,
        "both '{name}' and '{fallback}' are taken in this project"
    );
    Ok(fallback)
}
