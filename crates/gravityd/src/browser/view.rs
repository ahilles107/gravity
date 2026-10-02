//! Showing a bot's browser in the app. A connection watches one bot at a
//! time: it gets `browser_tabs` whenever the bot's tabs change and
//! `browser_frame` with each new screen of the tab it shows. That tab is the
//! one the bot used last, unless the owner picked another to look at.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::AppState;

use super::cdp::{self, Tab};
use super::BotBrowser;

/// How often the tab list is checked: tabs opening, closing, navigating.
const POLL: Duration = Duration::from_secs(1);

/// The profile directory of a bot's browser, when the bot runs here.
pub fn profile(app: &AppState, bot: &bus::Bot) -> Option<std::path::PathBuf> {
    if bot.is_linked() {
        return None;
    }
    let project = app.db.get_project(&bot.project_id).ok()??;
    let root = crate::paths::bot_dir(&app.cfg, &project.dir_name, &bot.dir_name);
    Some(BotBrowser::new(&root).profile())
}

/// The bot's open tabs, or none when its browser is not running.
async fn open_tabs(profile: &std::path::Path) -> Vec<Tab> {
    match cdp::port(profile) {
        Some(port) => cdp::tabs(port).await.unwrap_or_default(),
        None => Vec::new(),
    }
}

/// The screencast of the tab on show. Dropping it stops the stream, so an
/// aborted watch never leaves a tab streaming to a connection that moved on.
struct Showing {
    tab_id: String,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Showing {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Watches a bot's browser for one connection until the task is aborted.
pub async fn watch(
    app: Arc<AppState>,
    bot: bus::Bot,
    chosen: Option<String>,
    out: UnboundedSender<Value>,
) {
    let Some(profile) = profile(&app, &bot) else {
        let _ = out.send(json!({
            "type": "browser_tabs", "bot_id": bot.id, "open": false, "tabs": [],
            "active": null, "reason": "this bot runs on another machine"
        }));
        return;
    };
    let mut sent: Option<Value> = None;
    let mut showing: Option<Showing> = None;
    loop {
        let tabs = open_tabs(&profile).await;
        let active = chosen
            .as_ref()
            .and_then(|id| tabs.iter().find(|t| &t.id == id))
            .or_else(|| tabs.first())
            .cloned();
        let listing = json!({
            "type": "browser_tabs", "bot_id": bot.id, "open": !tabs.is_empty(),
            "active": active.as_ref().map(|t| t.id.clone()),
            "following": chosen.is_none(),
            "tabs": tabs.iter().map(|t| json!({ "id": t.id, "title": t.title, "url": t.url })).collect::<Vec<_>>()
        });
        if sent.as_ref() != Some(&listing) {
            if out.send(listing.clone()).is_err() {
                break;
            }
            sent = Some(listing);
        }
        let stale = match (&showing, &active) {
            (Some(on), Some(tab)) => on.tab_id != tab.id || on.task.is_finished(),
            (Some(_), None) => true,
            (None, _) => false,
        };
        if stale {
            showing = None;
        }
        if let (None, Some(tab)) = (&showing, active) {
            let (out, bot_id) = (out.clone(), bot.id.clone());
            let id = tab.id.clone();
            let task = tokio::spawn(async move {
                let result = cdp::screencast(&tab, |data, width, height| {
                    out.send(json!({
                        "type": "browser_frame", "bot_id": bot_id, "tab_id": tab.id,
                        "data": data, "width": width, "height": height
                    }))
                    .is_ok()
                })
                .await;
                if let Err(e) = result {
                    tracing::debug!(error = %e, "browser screencast ended");
                }
            });
            showing = Some(Showing { tab_id: id, task });
        }
        tokio::time::sleep(POLL).await;
    }
}
