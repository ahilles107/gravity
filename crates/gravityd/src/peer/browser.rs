//! A linked bot's browser, watched from the other machine.
//!
//! The daemon a bot runs on watches the bot's browser for a peer that asks
//! (`browser_watch`), through the same shared stream its own clients read,
//! and forwards what it would push to a client as `browser_feed` events. The
//! other daemon keeps one feed per stand-in and tab choice, shared by all its
//! viewers (the desktop, a phone), and renames the bot to the stand-in.
//!
//! Frames cross the link newest-only, at most a few a second, so a slow link
//! never queues them up; tab lists go straight through.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use bus::{Bot, Peer};
use serde_json::{json, Value};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::app::AppState;

/// Fewest milliseconds between two frames sent over the link.
const FRAME_INTERVAL: Duration = Duration::from_millis(150);

type Key = (String, Option<String>);

/// Browser feeds this daemon serves to peers, and feeds it reads from them.
#[derive(Default)]
pub struct Browsers {
    /// (peer, feed id) → the task watching and forwarding.
    served: Mutex<HashMap<(String, String), JoinHandle<()>>>,
    /// (stand-in, chosen tab) → the feed its viewers share.
    read: Mutex<HashMap<Key, Weak<Feed>>>,
}

impl Browsers {
    fn served(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), JoinHandle<()>>> {
        self.served.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn read(&self) -> std::sync::MutexGuard<'_, HashMap<Key, Weak<Feed>>> {
        self.read.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// How many bot browsers this daemon is watching for peers.
    pub fn serving(&self) -> usize {
        self.served()
            .values()
            .filter(|task| !task.is_finished())
            .count()
    }

    /// The live feeds read from a peer.
    fn feeds_from(&self, peer_id: &str) -> Vec<Arc<Feed>> {
        self.read()
            .values()
            .filter_map(Weak::upgrade)
            .filter(|feed| feed.peer_id == peer_id)
            .collect()
    }

    /// The link to a peer went down: what it served stops, and viewers of its
    /// bots are told the machine is offline until it comes back.
    pub fn link_down(&self, app: &AppState, peer_id: &str) {
        self.served().retain(|(peer, _), task| {
            let keep = peer != peer_id;
            if !keep {
                task.abort();
            }
            keep
        });
        let machine = app
            .db
            .get_peer(peer_id)
            .ok()
            .flatten()
            .map_or_else(|| "its machine".to_string(), |p| p.name);
        for feed in self.feeds_from(peer_id) {
            feed.tabs.send_replace(Some(json!({
                "type": "browser_tabs", "open": false, "tabs": [], "active": null,
                "reason": format!("{machine} is offline; its browser shows here once it is back")
            })));
        }
    }
}

// ---- serving this daemon's bots ----

/// Starts watching a bot's browser for the peer, forwarding as `browser_feed`.
pub(super) fn serve_watch(
    app: &Arc<AppState>,
    peer: &Peer,
    frame: &Value,
) -> anyhow::Result<Value> {
    let bot_id = frame["bot_id"].as_str().unwrap_or_default();
    let feed_id = frame["feed_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("'feed_id' is required"))?
        .to_string();
    let bot = app
        .db
        .get_live_bot(bot_id)?
        .filter(|b| !b.is_linked())
        .ok_or_else(|| anyhow::anyhow!("no bot with id {bot_id} runs here"))?;
    anyhow::ensure!(
        app.db.is_exposed_to_peer(&peer.id, &bot.id)?,
        "{} is not linked to this daemon",
        bot.name
    );
    let tab = frame["tab_id"].as_str().map(str::to_string);
    let (tx, rx) = mpsc::unbounded_channel();
    let watcher = crate::browser::view::watch(app.clone(), bot, tab, tx);
    let relay = relay(app.peers.clone(), peer.id.clone(), feed_id.clone(), rx);
    let task = tokio::spawn(async move {
        tokio::join!(watcher, relay);
    });
    if let Some(old) = app
        .peers
        .browsers
        .served()
        .insert((peer.id.clone(), feed_id), task)
    {
        old.abort();
    }
    Ok(json!({}))
}

/// Forwards a watch's pushes to the peer: tab lists at once, frames newest
/// only and spaced out.
async fn relay(
    hub: super::PeerHub,
    peer_id: String,
    feed_id: String,
    mut rx: mpsc::UnboundedReceiver<Value>,
) {
    let mut pending: Option<Value> = None;
    let mut tick = tokio::time::interval(FRAME_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            push = rx.recv() => {
                let Some(push) = push else { break };
                if push["type"] == "browser_frame" {
                    pending = Some(push);
                } else {
                    hub.notify(&peer_id, json!({ "type": "browser_feed", "feed_id": feed_id, "push": push }));
                }
            }
            _ = tick.tick(), if pending.is_some() => {
                if let Some(push) = pending.take() {
                    hub.notify(&peer_id, json!({ "type": "browser_feed", "feed_id": feed_id, "push": push }));
                }
            }
        }
    }
}

/// The peer stopped watching.
pub(super) fn serve_unwatch(app: &AppState, peer: &Peer, frame: &Value) {
    let feed_id = frame["feed_id"].as_str().unwrap_or_default().to_string();
    if let Some(task) = app
        .peers
        .browsers
        .served()
        .remove(&(peer.id.clone(), feed_id))
    {
        task.abort();
    }
}

// ---- watching a peer's bot ----

/// One stand-in's browser as its machine feeds it, shared by every viewer
/// here that chose the same tab. Dropping the last stops the feed.
struct Feed {
    id: String,
    peer_id: String,
    remote_bot_id: String,
    tab: Option<String>,
    tabs: watch::Sender<Option<Value>>,
    frames: watch::Sender<Option<Value>>,
    app: Arc<AppState>,
}

impl Drop for Feed {
    fn drop(&mut self) {
        self.app.peers.notify(
            &self.peer_id,
            json!({ "type": "browser_unwatch", "feed_id": self.id }),
        );
    }
}

impl Feed {
    async fn ask(&self) -> anyhow::Result<()> {
        let mut request = json!({
            "type": "browser_watch", "bot_id": self.remote_bot_id, "feed_id": self.id
        });
        if let Some(tab) = &self.tab {
            request["tab_id"] = json!(tab);
        }
        self.app.peers.request(&self.peer_id, request).await?;
        Ok(())
    }
}

/// The feed of a stand-in's browser for this tab choice, asking the peer for
/// it if nobody here watches it yet.
async fn feed(
    app: &Arc<AppState>,
    stand_in: &Bot,
    tab: Option<String>,
) -> anyhow::Result<Arc<Feed>> {
    let (Some(peer_id), Some(remote)) = (&stand_in.peer_id, &stand_in.remote_bot_id) else {
        anyhow::bail!("{} runs on this machine", stand_in.name);
    };
    let key = (stand_in.id.clone(), tab.clone());
    let fresh = {
        let mut read = app.peers.browsers.read();
        read.retain(|_, feed| feed.strong_count() > 0);
        if let Some(feed) = read.get(&key).and_then(Weak::upgrade) {
            return Ok(feed);
        }
        let feed = Arc::new(Feed {
            id: bus::new_id(),
            peer_id: peer_id.clone(),
            remote_bot_id: remote.clone(),
            tab,
            tabs: watch::channel(None).0,
            frames: watch::channel(None).0,
            app: app.clone(),
        });
        read.insert(key, Arc::downgrade(&feed));
        feed
    };
    fresh.ask().await?;
    Ok(fresh)
}

/// Watches a linked bot's browser for one connection, through its machine.
pub async fn watch(
    app: Arc<AppState>,
    stand_in: Bot,
    tab: Option<String>,
    out: mpsc::UnboundedSender<Value>,
) {
    let feed = match feed(&app, &stand_in, tab).await {
        Ok(feed) => feed,
        Err(e) => {
            let _ = out.send(json!({
                "type": "browser_tabs", "bot_id": stand_in.id, "open": false, "tabs": [],
                "active": null, "reason": format!("{e:#}")
            }));
            return;
        }
    };
    let (mut tabs, mut frames) = (feed.tabs.subscribe(), feed.frames.subscribe());
    tabs.mark_changed();
    frames.mark_changed();
    let local = |push: &Option<Value>| {
        push.clone().map(|mut push| {
            push["bot_id"] = json!(stand_in.id);
            push
        })
    };
    loop {
        let push = tokio::select! {
            changed = tabs.changed() => match changed {
                Ok(()) => local(&tabs.borrow_and_update()),
                Err(_) => break,
            },
            changed = frames.changed() => match changed {
                Ok(()) => local(&frames.borrow_and_update()),
                Err(_) => break,
            },
        };
        if let Some(push) = push {
            if out.send(push).is_err() {
                break;
            }
        }
    }
    drop(feed);
}

/// What a peer's watch pushed, for the feed it belongs to.
pub(super) fn receive(app: &AppState, peer_id: &str, frame: &Value) {
    let feed_id = frame["feed_id"].as_str().unwrap_or_default();
    let push = &frame["push"];
    let Some(feed) = app
        .peers
        .browsers
        .feeds_from(peer_id)
        .into_iter()
        .find(|feed| feed.id == feed_id)
    else {
        return;
    };
    let channel = if push["type"] == "browser_frame" {
        &feed.frames
    } else {
        &feed.tabs
    };
    channel.send_replace(Some(push.clone()));
}

/// Asks again for the feeds still watched here, once the link is back.
pub(super) async fn link_up(app: Arc<AppState>, peer_id: String) {
    for feed in app.peers.browsers.feeds_from(&peer_id) {
        if let Err(e) = feed.ask().await {
            tracing::info!(peer_id, error = %e, "peer browser not resumed");
        }
    }
}
