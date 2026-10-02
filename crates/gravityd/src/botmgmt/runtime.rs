//! Runtime selection shared by the control client and bot-authenticated MCP.

use std::sync::Arc;

use bus::{Bot, BotRuntime};
use serde_json::Value;

use crate::app::AppState;
use crate::events::Push;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct RuntimeUnavailable(String);

pub fn requested_runtime(args: &Value) -> anyhow::Result<Option<BotRuntime>> {
    args.get("runtime")
        .map(|value| serde_json::from_value(value.clone()).map_err(anyhow::Error::from))
        .transpose()
}

/// Preflight before changing any settings or stopping a working session.
pub fn check_runtime_available(app: &Arc<AppState>, runtime: BotRuntime) -> anyhow::Result<()> {
    app.supervisor
        .adapter()
        .check_available(runtime)
        .map_err(|error| RuntimeUnavailable(format!("{error:#}")).into())
}

/// Persist a provider change and restart, preserving the workspace and histories.
/// Allows or forbids the bot the owner's own Chrome. The flag is read at
/// spawn, so a running session restarts to pick it up.
pub fn set_bot_user_chrome(app: &Arc<AppState>, bot: &Bot, enabled: bool) -> anyhow::Result<Bot> {
    anyhow::ensure!(
        !bot.is_linked(),
        "{} runs on another machine; change it there",
        bot.name
    );
    if bot.user_chrome == enabled {
        return Ok(bot.clone());
    }
    app.db.set_bot_user_chrome(&bot.id, enabled)?;
    let mut updated = bot.clone();
    updated.user_chrome = enabled;
    // The system prompt says which browsers the bot has.
    if let Err(e) = super::reprovision(app, &updated) {
        tracing::warn!(bot_id = %bot.id, error = %e, "system prompt not refreshed");
    }
    if let Err(error) = app.supervisor.restart_bot(&bot.id) {
        app.db.set_bot_user_chrome(&bot.id, bot.user_chrome)?;
        return Err(error);
    }
    app.events.push(Push::BotUpdated {
        bot: updated.clone(),
    });
    Ok(updated)
}

pub fn set_bot_runtime(app: &Arc<AppState>, bot: &Bot, runtime: BotRuntime) -> anyhow::Result<Bot> {
    if bot.runtime == runtime {
        return Ok(bot.clone());
    }
    check_runtime_available(app, runtime)?;
    app.db.set_bot_runtime(&bot.id, runtime)?;
    if let Err(error) = app.supervisor.restart_bot(&bot.id) {
        app.db.set_bot_runtime(&bot.id, bot.runtime)?;
        return Err(error);
    }
    let mut updated = bot.clone();
    updated.runtime = runtime;
    app.events.push(Push::BotUpdated {
        bot: updated.clone(),
    });
    Ok(updated)
}
