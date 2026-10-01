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
