//! A bot's conversation read from its runtime's transcript, for the chat
//! pane. See `docs/superpowers/specs/2026-09-16-chat-pane-design.md`.

use std::sync::Arc;
use std::time::Duration;

use crate::app::AppState;
use crate::events::Push;

mod browser_steps;
mod builder;
#[cfg(test)]
mod builder_tests;
pub mod commands;
mod envelope;
pub mod files;
pub mod model;
mod steps;
mod store;
mod triggers;

pub(crate) use envelope::unwrap_peer;
pub(crate) use steps::truncate;
pub use store::ChatStore;

/// How often the watcher looks for transcript growth in loaded chats.
const WATCH_INTERVAL: Duration = Duration::from_secs(1);

/// Keeps every loaded chat current and pushes the turns that change, so an
/// open chat follows the bot as it works.
pub async fn watch(app: Arc<AppState>) {
    let mut tick = tokio::time::interval(WATCH_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let app = app.clone();
        let _ = tokio::task::spawn_blocking(move || {
            for bot_id in app.chat.loaded() {
                let Ok(Some(bot)) = app.db.get_live_bot(&bot_id) else {
                    continue;
                };
                match app.chat.refresh(&app, &bot) {
                    Ok(turns) if !turns.is_empty() => {
                        app.events.push(Push::ChatTurns { bot_id, turns });
                    }
                    Ok(_) => {}
                    Err(e) => tracing::debug!(bot_id, error = %e, "chat refresh failed"),
                }
            }
        })
        .await;
    }
}
