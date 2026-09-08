//! Per-bot runtime supervision: state machine, terminal buffers, crash
//! restart with backoff, and channel delivery through each session's inbox
//! socket.
//!
//! Bots are always-on. Nothing starts them by hand: the daemon reconciles
//! every live bot into a running session at boot and on every supervision
//! tick, so the only way a bot is not running is that it just crashed and is
//! waiting out its backoff.
//!
//! Terminal input goes straight to the PTY (grant-gated at the control
//! plane); bus deliveries go through the inbox socket and never touch the
//! terminal, so they cannot interleave with the user's typing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context;
use bus::BotState;
use tokio::runtime::Handle;
use tokio::task::AbortHandle;

use crate::channel::MsgSocket;
use crate::config::Config;
use crate::db::Db;
use crate::events::{Events, Internal, Push};
use crate::overrides::AutoCompactOverride;
use crate::runtime::{BotSpec, RuntimeAdapter, RuntimeSession, SessionEvent};
use crate::secrets::Secrets;
use crate::terminal::TermBuffer;

mod hooks;
mod lifecycle;
mod termio;

pub const BOT_TOKEN_ENV: &str = "GRAVITY_TOKEN";
/// Caps how large a bot's conversation grows before Claude Code compacts it.
/// Set from the daemon rather than the workspace settings so a bot editing its
/// own `.claude` cannot opt itself into an unbounded (and costly) window.
pub const AUTO_COMPACT_WINDOW_ENV: &str = "CLAUDE_CODE_AUTO_COMPACT_WINDOW";
/// Age threshold past which `claude --continue` interposes a "resume from
/// summary?" chooser before resuming. Bots are one continuous conversation, so
/// the daemon pushes it out of reach: a restart must land back in the full
/// session, not at an interactive dialog nothing will answer. The auto-compact
/// window above remains the cost backstop.
pub const RESUME_THRESHOLD_MINUTES_ENV: &str = "CLAUDE_CODE_RESUME_THRESHOLD_MINUTES";
/// Size half of the same chooser, which needs both thresholds to trip.
pub const RESUME_TOKEN_THRESHOLD_ENV: &str = "CLAUDE_CODE_RESUME_TOKEN_THRESHOLD";
/// A year of minutes and a billion tokens: past anything a session reaches,
/// but plainly in range. An out-of-range value would be ignored by the CLI and
/// silently restore the default that puts the chooser back — the same trap
/// `Config::effective_auto_compact_window` clamps against.
pub const RESUME_THRESHOLD_MINUTES: u32 = 525_600;
pub const RESUME_TOKEN_THRESHOLD: u32 = 1_000_000_000;
/// Keeps Claude Code 2.1.263+ off its alternate-screen renderer, which leaves
/// xterm without scrollback and blank between screens (boot, restart).
pub const DISABLE_ALTERNATE_SCREEN_ENV: &str = "CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN";
/// Stops Claude Code claiming the mouse, so the wheel scrolls xterm again.
pub const DISABLE_MOUSE_ENV: &str = "CLAUDE_CODE_DISABLE_MOUSE";
/// First crash-restart delay; doubles per consecutive crash up to `MAX_BACKOFF`.
const BASE_BACKOFF: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(300);
/// A resumed session that dies faster than this is treated as a transcript the
/// runtime could not load, so the next start comes up fresh.
const RESUME_FAST_EXIT: Duration = Duration::from_secs(10);
/// Consecutive crashes after which a pinned model is treated as one the
/// runtime will not accept, and the pin is dropped so the bot can start at
/// all. A model id read from a transcript was real when it was written, but a
/// retired one would otherwise crash-loop a bot nothing can start by hand.
/// Higher than one so a session the user themselves ended keeps its pin.
const MODEL_UNPIN_CRASHES: u32 = 3;
/// How long a forced-repaint nudge stays at the shrunken size before being
/// restored. Long enough for the runtime to observe the intermediate size —
/// SIGWINCH is a bit, not a queue, so an instant restore coalesces both
/// signals into one wakeup that reads the final (unchanged) size and repaints
/// nothing.
const REPAINT_NUDGE_DELAY: Duration = Duration::from_millis(50);

/// Why a delivery could not be handed to the runtime right now.
#[derive(Debug)]
pub enum DeliverError {
    /// Transient: no session, or the inbox socket is not known yet. The
    /// delivery worker defers without burning a retry attempt.
    NotReady(String),
    /// The send itself failed; counts as a real attempt.
    Failed(anyhow::Error),
}

impl std::fmt::Display for DeliverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeliverError::NotReady(r) => write!(f, "not ready: {r}"),
            DeliverError::Failed(e) => write!(f, "send failed: {e}"),
        }
    }
}

struct BotHandle {
    state: BotState,
    reason: String,
    session: Option<Arc<Mutex<Box<dyn RuntimeSession>>>>,
    term: Arc<TermBuffer>,
    /// Inbox socket for channel delivery; from the adapter (double) or the
    /// SessionStart hook (pty).
    msg_socket: Option<MsgSocket>,
    /// Last size applied to the runtime, so an attach can tell a real resize
    /// from one that would be a no-op.
    size: (u16, u16),
    /// The one pending restore after a forced repaint nudge, if any.
    repaint_restore: Option<AbortHandle>,
    /// Invalidates a restore that races with a newer resize.
    repaint_generation: u64,
    /// Set when the daemon asked for the stop (archival), so exit is not a
    /// crash and the reconciler leaves the bot alone.
    stopping: bool,
    consecutive_crashes: u32,
    last_start: Instant,
    /// Earliest time the reconciler may start this bot again; set while a
    /// crash backoff is in flight.
    next_start_at: Option<Instant>,
    /// Whether the current session was started with `--continue`.
    resumed: bool,
    /// Set when a resumed session died on its face, so the next start opens a
    /// fresh conversation rather than crash-looping on a transcript the
    /// runtime cannot load.
    skip_resume: bool,
    /// Whether the current session was started with an explicit `--model`.
    /// A crash streak under a pin is what condemns the pin itself.
    pinned_model: bool,
}

impl BotHandle {
    fn new(scrollback: usize) -> Self {
        Self {
            state: BotState::Stopped,
            reason: String::new(),
            session: None,
            term: Arc::new(TermBuffer::new(scrollback)),
            msg_socket: None,
            size: (0, 0),
            repaint_restore: None,
            repaint_generation: 0,
            stopping: false,
            consecutive_crashes: 0,
            last_start: Instant::now(),
            next_start_at: None,
            resumed: false,
            skip_resume: false,
            pinned_model: false,
        }
    }
}

#[derive(Clone)]
pub struct Supervisor {
    inner: Arc<SupervisorInner>,
}

struct SupervisorInner {
    adapter: Arc<dyn RuntimeAdapter>,
    cfg: Config,
    db: Db,
    events: Events,
    secrets: Arc<Secrets>,
    auto_compact: AutoCompactOverride,
    bots: Mutex<HashMap<String, BotHandle>>,
}

impl Supervisor {
    pub fn new(
        adapter: Arc<dyn RuntimeAdapter>,
        cfg: Config,
        db: Db,
        events: Events,
        secrets: Arc<Secrets>,
        auto_compact: AutoCompactOverride,
    ) -> Self {
        Self {
            inner: Arc::new(SupervisorInner {
                adapter,
                cfg,
                db,
                events,
                secrets,
                auto_compact,
                bots: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn adapter(&self) -> &Arc<dyn RuntimeAdapter> {
        &self.inner.adapter
    }

    fn lock_bots(&self) -> std::sync::MutexGuard<'_, HashMap<String, BotHandle>> {
        self.inner.bots.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn state(&self, bot_id: &str) -> (BotState, String) {
        let bots = self.lock_bots();
        bots.get(bot_id)
            .map(|h| (h.state, h.reason.clone()))
            .unwrap_or((BotState::Stopped, String::new()))
    }

    pub fn set_state(&self, bot_id: &str, state: BotState, reason: &str) {
        {
            let mut bots = self.lock_bots();
            let Some(h) = bots.get_mut(bot_id) else {
                return;
            };
            if h.state == state {
                return;
            }
            h.state = state;
            h.reason = reason.to_string();
        }
        tracing::info!(bot_id, state = state.as_str(), reason, "bot state");
        self.inner.events.push(Push::BotState {
            bot_id: bot_id.to_string(),
            state,
            reason: reason.to_string(),
            at: chrono::Utc::now().to_rfc3339(),
        });
    }

    /// True while the bot's runtime process is up, whatever the session
    /// inside it is doing.
    fn has_session(&self, bot_id: &str) -> bool {
        let bots = self.lock_bots();
        bots.get(bot_id).is_some_and(|h| h.session.is_some())
    }

    pub fn active_total(&self) -> usize {
        let bots = self.lock_bots();
        bots.values().filter(|h| h.session.is_some()).count()
    }

    pub fn stop_bot(&self, bot_id: &str) -> anyhow::Result<()> {
        let session = {
            let mut bots = self.lock_bots();
            let Some(h) = bots.get_mut(bot_id) else {
                return Ok(());
            };
            h.stopping = true;
            h.session.clone()
        };
        if let Some(session) = session {
            self.set_state(bot_id, BotState::Stopping, "stop requested");
            session.lock().unwrap_or_else(|e| e.into_inner()).kill()?;
        } else {
            self.set_state(bot_id, BotState::Stopped, "not running");
        }
        Ok(())
    }

    /// Record the session's inbox socket, reported by the SessionStart hook.
    pub fn set_msg_socket(&self, bot_id: &str, path: &str, token: Option<&str>) {
        if path.is_empty() {
            return;
        }
        let mut bots = self.lock_bots();
        if let Some(h) = bots.get_mut(bot_id) {
            h.msg_socket = Some(MsgSocket {
                path: std::path::PathBuf::from(path),
                token: token.filter(|t| !t.is_empty()).map(|t| t.to_string()),
            });
            tracing::info!(bot_id, socket = path, "inbox socket registered");
        }
    }

    /// Deliver a rendered envelope through the session's inbox socket. The
    /// message is read between tool calls or starts a new turn when the
    /// session is idle; it never touches the terminal.
    pub fn deliver(&self, bot_id: &str, text: &str) -> Result<(), DeliverError> {
        let socket = {
            let bots = self.lock_bots();
            let Some(h) = bots.get(bot_id) else {
                return Err(DeliverError::NotReady("bot has no runtime".to_string()));
            };
            if h.session.is_none() || !h.state.is_running() {
                return Err(DeliverError::NotReady(format!(
                    "bot is {}",
                    h.state.as_str()
                )));
            }
            match &h.msg_socket {
                Some(s) => s.clone(),
                None => {
                    return Err(DeliverError::NotReady(
                        "inbox socket not reported yet".to_string(),
                    ));
                }
            }
        };
        crate::channel::send(&socket, text).map_err(DeliverError::Failed)
    }
}
