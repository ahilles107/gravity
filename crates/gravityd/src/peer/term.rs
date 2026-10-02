//! A linked bot's terminal across the peer link, to watch and type into.
//!
//! The daemon a bot runs on feeds its terminal output to a peer that asks
//! (`term_attach`), as `term_frames` events, until told to stop. The other
//! daemon keeps a mirror of it: the frames go into the stand-in's own
//! terminal buffer, so its clients (the desktop, a phone) attach to the
//! stand-in exactly as to a local bot, with replay and resume, and the link
//! carries one feed however many of them watch. Input and resizes go back as
//! `term_input` and `term_resize` events. The feed runs while anyone watches,
//! resumes where it left off after the link drops, and stops when the last
//! viewer leaves.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use bus::{Bot, Peer};
use serde_json::{json, Value};
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

use crate::app::AppState;
use crate::terminal::coalesce;

/// Largest `term_frames` event, in output bytes.
const MAX_FRAME_BYTES: usize = 64 * 1024;

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn unb64(text: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .unwrap_or_default()
}

/// Where a mirror stands.
enum State {
    /// No feed: nobody watches, or the link is down.
    Stopped,
    /// Asked for a feed; frames that overtake the reply wait here.
    Starting(Vec<(u64, Vec<u8>)>),
    Running,
}

/// A peer bot's terminal mirrored into its stand-in's buffer.
struct Mirror {
    peer_id: String,
    remote_bot_id: String,
    viewers: usize,
    state: State,
    /// The newest of the peer's sequence numbers pushed into the mirror.
    last_remote: u64,
}

/// Terminal feeds this daemon serves, and mirrors of its peers' terminals.
#[derive(Default)]
pub struct Terms {
    /// (peer, local bot) → the task feeding that bot's terminal to the peer.
    feeds: Mutex<HashMap<(String, String), JoinHandle<()>>>,
    /// Stand-in bot id → its mirror.
    mirrors: Mutex<HashMap<String, Mirror>>,
}

impl Terms {
    fn feeds(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), JoinHandle<()>>> {
        self.feeds.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn mirrors(&self) -> std::sync::MutexGuard<'_, HashMap<String, Mirror>> {
        self.mirrors.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// How many bot terminals this daemon is feeding to peers.
    pub fn feeding(&self) -> usize {
        self.feeds()
            .values()
            .filter(|task| !task.is_finished())
            .count()
    }

    /// The link to a peer went down: its feeds stop, and mirrors of its bots
    /// wait for the link to come back.
    pub fn link_down(&self, peer_id: &str) {
        self.feeds().retain(|(peer, _), task| {
            let keep = peer != peer_id;
            if !keep {
                task.abort();
            }
            keep
        });
        for mirror in self.mirrors().values_mut() {
            if mirror.peer_id == peer_id {
                mirror.state = State::Stopped;
            }
        }
    }
}

// ---- serving this daemon's bots ----

/// The bot a peer means, which must run here and be linked to it.
fn exposed_bot(app: &AppState, peer: &Peer, frame: &Value) -> anyhow::Result<Bot> {
    let bot_id = frame["bot_id"].as_str().unwrap_or_default();
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
    Ok(bot)
}

/// Starts feeding a bot's terminal to the peer, and answers with what it
/// already holds after `after_seq`.
pub(super) fn serve_attach(
    app: &Arc<AppState>,
    peer: &Peer,
    frame: &Value,
) -> anyhow::Result<Value> {
    let bot = exposed_bot(app, peer, frame)?;
    let after_seq = frame["after_seq"].as_u64().unwrap_or(0);
    let term = app.supervisor.ensure_term(&bot.id);
    // Subscribed before the replay, as a local attach does, so nothing lands
    // between the two.
    let mut rx = term.subscribe();
    let replay = term.replay_after(after_seq);
    let (hub, peer_id, bot_id) = (app.peers.clone(), peer.id.clone(), bot.id.clone());
    let mut last = replay.latest;
    let feed_term = term.clone();
    let feed = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(frame) if frame.seq <= last => continue,
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
            for frame in coalesce(feed_term.newer_than(last).frames, MAX_FRAME_BYTES) {
                last = frame.seq;
                hub.notify(
                    &peer_id,
                    json!({ "type": "term_frames", "bot_id": bot_id, "seq": frame.seq, "data": b64(&frame.data) }),
                );
            }
        }
    });
    if let Some(old) = app
        .peers
        .terms
        .feeds()
        .insert((peer.id.clone(), bot.id.clone()), feed)
    {
        old.abort();
    }
    let frames: Vec<Value> = coalesce(replay.frames, MAX_FRAME_BYTES)
        .iter()
        .map(|f| json!({ "seq": f.seq, "data": b64(&f.data) }))
        .collect();
    Ok(json!({ "latest": replay.latest, "resumed": replay.resumed, "frames": frames }))
}

/// The peer stopped watching a bot's terminal.
pub(super) fn serve_detach(app: &AppState, peer: &Peer, frame: &Value) -> anyhow::Result<Value> {
    let bot_id = frame["bot_id"].as_str().unwrap_or_default().to_string();
    if let Some(feed) = app.peers.terms.feeds().remove(&(peer.id.clone(), bot_id)) {
        feed.abort();
    }
    Ok(json!({}))
}

/// The peer's owner restarts a bot linked to it, or clears its conversation.
pub(super) fn serve_session(app: &AppState, peer: &Peer, frame: &Value) -> anyhow::Result<Value> {
    let bot = exposed_bot(app, peer, frame)?;
    if frame["type"] == "clear_bot_session" {
        let root = std::path::Path::new(&bot.workspace_path).parent();
        app.supervisor.clear_session(&bot.id, root)?;
    } else {
        app.supervisor.restart_bot(&bot.id)?;
    }
    tracing::info!(peer = %peer.name, bot = %bot.name, kind = %frame["type"], "peer restarted a bot");
    Ok(json!({}))
}

/// Typing and resizes from the peer's clients, for a bot linked to it.
pub(super) fn serve_event(app: &AppState, peer: &Peer, frame: &Value) {
    let Ok(bot) = exposed_bot(app, peer, frame) else {
        return;
    };
    match frame["type"].as_str().unwrap_or_default() {
        "term_input" => {
            let data = frame["data"].as_str().unwrap_or_default();
            if let Err(e) = app.supervisor.input(&bot.id, data.as_bytes()) {
                tracing::debug!(bot = %bot.name, error = %e, "peer input not delivered");
            }
        }
        "term_detach" => {
            let _ = serve_detach(app, peer, frame);
        }
        "term_resize" => {
            let size = |key: &str, default: u64| frame[key].as_u64().unwrap_or(default) as u16;
            let force = frame["force"].as_bool().unwrap_or(false);
            let _ = app
                .supervisor
                .resize(&bot.id, size("cols", 120), size("rows", 36), force);
        }
        _ => {}
    }
}

// ---- mirroring a peer's bot ----

/// One client watching a stand-in's terminal. While any exists the peer
/// feeds the mirror; dropping the last stops the feed.
pub struct Viewer {
    app: Arc<AppState>,
    bot_id: String,
}

impl Drop for Viewer {
    fn drop(&mut self) {
        let mut mirrors = self.app.peers.terms.mirrors();
        let Some(mirror) = mirrors.get_mut(&self.bot_id) else {
            return;
        };
        mirror.viewers = mirror.viewers.saturating_sub(1);
        if mirror.viewers > 0 {
            return;
        }
        let was_feeding = !matches!(mirror.state, State::Stopped);
        mirror.state = State::Stopped;
        if was_feeding {
            self.app.peers.notify(
                &mirror.peer_id,
                json!({ "type": "term_detach", "bot_id": mirror.remote_bot_id }),
            );
        }
    }
}

/// Starts watching a linked bot's terminal: its mirror is fed from the peer
/// while the returned viewer lives.
pub async fn view(app: &Arc<AppState>, stand_in: &Bot) -> anyhow::Result<Viewer> {
    let (Some(peer_id), Some(remote)) = (&stand_in.peer_id, &stand_in.remote_bot_id) else {
        anyhow::bail!("{} runs on this machine", stand_in.name);
    };
    let start = {
        let mut mirrors = app.peers.terms.mirrors();
        let mirror = mirrors
            .entry(stand_in.id.clone())
            .or_insert_with(|| Mirror {
                peer_id: peer_id.clone(),
                remote_bot_id: remote.clone(),
                viewers: 0,
                state: State::Stopped,
                last_remote: 0,
            });
        mirror.viewers += 1;
        matches!(mirror.state, State::Stopped)
    };
    let viewer = Viewer {
        app: app.clone(),
        bot_id: stand_in.id.clone(),
    };
    if start {
        feed(app, &stand_in.id).await?;
    }
    Ok(viewer)
}

/// Asks the peer to feed a mirror, picking up after the last frame it sent.
async fn feed(app: &Arc<AppState>, bot_id: &str) -> anyhow::Result<()> {
    let (peer_id, remote, after_seq) = {
        let mut mirrors = app.peers.terms.mirrors();
        let mirror = mirrors
            .get_mut(bot_id)
            .ok_or_else(|| anyhow::anyhow!("no mirror"))?;
        mirror.state = State::Starting(Vec::new());
        (
            mirror.peer_id.clone(),
            mirror.remote_bot_id.clone(),
            mirror.last_remote,
        )
    };
    let request = json!({ "type": "term_attach", "bot_id": remote, "after_seq": after_seq });
    let reply = match app.peers.request(&peer_id, request).await {
        Ok(reply) => reply,
        Err(e) => {
            if let Some(mirror) = app.peers.terms.mirrors().get_mut(bot_id) {
                mirror.state = State::Stopped;
            }
            return Err(e.into());
        }
    };
    let term = app.supervisor.ensure_term(bot_id);
    let mut mirrors = app.peers.terms.mirrors();
    let Some(mirror) = mirrors.get_mut(bot_id) else {
        return Ok(());
    };
    let early = match std::mem::replace(&mut mirror.state, State::Running) {
        State::Starting(early) => early,
        // Everyone left while the peer was answering.
        State::Stopped => {
            mirror.state = State::Stopped;
            return Ok(());
        }
        State::Running => Vec::new(),
    };
    let replayed = reply["frames"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| Some((f["seq"].as_u64()?, unb64(f["data"].as_str()?))));
    for (seq, data) in replayed.chain(early) {
        if seq > mirror.last_remote {
            term.push(data);
            mirror.last_remote = seq;
        }
    }
    Ok(())
}

/// Terminal output from a peer, for the mirrors of its bot.
pub(super) fn receive_frames(app: &AppState, peer_id: &str, frame: &Value) {
    let remote = frame["bot_id"].as_str().unwrap_or_default();
    let (Some(seq), Some(data)) = (frame["seq"].as_u64(), frame["data"].as_str()) else {
        return;
    };
    let Ok(stand_ins) = app.db.linked_bots_for(peer_id, remote) else {
        return;
    };
    let mut mirrors = app.peers.terms.mirrors();
    for bot in stand_ins {
        let Some(mirror) = mirrors.get_mut(&bot.id) else {
            continue;
        };
        match &mut mirror.state {
            State::Starting(early) => early.push((seq, unb64(data))),
            State::Running if seq > mirror.last_remote => {
                app.supervisor.ensure_term(&bot.id).push(unb64(data));
                mirror.last_remote = seq;
            }
            _ => {}
        }
    }
}

/// Feeds the mirrors someone is still watching, once the link is back.
pub(super) async fn link_up(app: Arc<AppState>, peer_id: String) {
    let waiting: Vec<String> = app
        .peers
        .terms
        .mirrors()
        .iter()
        .filter(|(_, m)| m.peer_id == peer_id && m.viewers > 0 && matches!(m.state, State::Stopped))
        .map(|(id, _)| id.clone())
        .collect();
    for bot_id in waiting {
        if let Err(e) = feed(&app, &bot_id).await {
            tracing::info!(bot_id, error = %e, "peer terminal not resumed");
        }
    }
}

/// Typing from a client here, for the real terminal on the peer.
pub fn input(app: &AppState, stand_in: &Bot, data: &str) {
    if let (Some(peer), Some(remote)) = (&stand_in.peer_id, &stand_in.remote_bot_id) {
        app.peers.notify(
            peer,
            json!({ "type": "term_input", "bot_id": remote, "data": data }),
        );
    }
}

/// A client's terminal size, for the real terminal on the peer.
pub fn resize(app: &AppState, stand_in: &Bot, cols: u16, rows: u16, force: bool) {
    if let (Some(peer), Some(remote)) = (&stand_in.peer_id, &stand_in.remote_bot_id) {
        app.peers.notify(
            peer,
            json!({ "type": "term_resize", "bot_id": remote, "cols": cols, "rows": rows, "force": force }),
        );
    }
}
