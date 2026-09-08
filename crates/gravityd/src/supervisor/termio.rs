//! The bot's terminal as the daemon exposes it: the shared buffer, raw input,
//! and resize (including the forced-repaint nudge).

use super::*;

impl Supervisor {
    pub fn term(&self, bot_id: &str) -> Option<Arc<TermBuffer>> {
        let bots = self.lock_bots();
        bots.get(bot_id).map(|h| h.term.clone())
    }

    /// Ensure a terminal buffer exists even before first start (for attach).
    pub fn ensure_term(&self, bot_id: &str) -> Arc<TermBuffer> {
        let mut bots = self.lock_bots();
        bots.entry(bot_id.to_string())
            .or_insert_with(|| BotHandle::new(self.inner.cfg.scrollback_bytes))
            .term
            .clone()
    }

    /// Forward raw terminal input. Grant checks happen at the control plane;
    /// there is no input lease — the terminal belongs to the user.
    pub fn input(&self, bot_id: &str, data: &[u8]) -> anyhow::Result<()> {
        let session = {
            let mut bots = self.lock_bots();
            let h = bots.get_mut(bot_id).context("unknown bot")?;
            h.session.clone().context("bot not running")?
        };
        let result = session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .send_input(data);
        result
    }

    /// Resize the runtime's terminal. With `force`, a resize to the size the
    /// terminal already has is turned into a real change first: TIOCSWINSZ
    /// only raises SIGWINCH when the dimensions differ, and a full-screen TUI
    /// repaints on SIGWINCH. The restore back to the true size waits for the
    /// runtime to have written something since the shrink (see
    /// [`wait_for_repaint`]) so it observes both changes rather than one
    /// coalesced no-op. Without the nudge a client that re-attaches at the
    /// same size sees only whatever replay could rebuild.
    pub fn resize(&self, bot_id: &str, cols: u16, rows: u16, force: bool) -> anyhow::Result<()> {
        let runtime = if force {
            Some(Handle::try_current().context("forced repaint requires an async runtime")?)
        } else {
            None
        };
        let (session, term, unchanged, repaint_generation, previous_restore) = {
            let mut bots = self.lock_bots();
            let h = bots.get_mut(bot_id).context("unknown bot")?;
            let session = h.session.clone().context("bot not running")?;
            let unchanged = h.size == (cols, rows);
            h.size = (cols, rows);
            h.repaint_generation = h.repaint_generation.wrapping_add(1);
            (
                session,
                h.term.clone(),
                unchanged,
                h.repaint_generation,
                h.repaint_restore.take(),
            )
        };
        if let Some(previous_restore) = previous_restore {
            previous_restore.abort();
        }
        if force && unchanged && rows > 1 {
            let quiet_since = term.latest_seq();
            {
                let mut guard = session.lock().unwrap_or_else(|e| e.into_inner());
                guard.resize(cols, rows - 1)?;
            }
            let Some(runtime) = runtime else {
                anyhow::bail!("forced repaint requires an async runtime");
            };
            let sup = self.clone();
            let bot_id = bot_id.to_string();
            let restore_bot_id = bot_id.clone();
            let restore = runtime.spawn(async move {
                wait_for_repaint(&term, quiet_since).await;
                sup.restore_nudged_size(&restore_bot_id, cols, rows, repaint_generation);
            });
            let abort = restore.abort_handle();
            let mut bots = self.lock_bots();
            match bots.get_mut(bot_id.as_str()) {
                Some(h) if h.repaint_generation == repaint_generation && h.size == (cols, rows) => {
                    h.repaint_restore = Some(abort);
                }
                _ => abort.abort(),
            }
            return Ok(());
        }
        let mut guard = session.lock().unwrap_or_else(|e| e.into_inner());
        guard.resize(cols, rows)
    }

    /// Second half of the forced-repaint nudge: put the pty back to the size
    /// the client asked for — unless a real resize claimed it meanwhile, in
    /// which case that size is the truth and this restore has nothing to do.
    fn restore_nudged_size(&self, bot_id: &str, cols: u16, rows: u16, repaint_generation: u64) {
        let session = {
            let mut bots = self.lock_bots();
            match bots.get_mut(bot_id) {
                Some(h) if h.size == (cols, rows) && h.repaint_generation == repaint_generation => {
                    h.repaint_restore = None;
                    h.session.clone()
                }
                _ => None,
            }
        };
        if let Some(session) = session {
            let mut guard = session.lock().unwrap_or_else(|e| e.into_inner());
            let _ = guard.resize(cols, rows);
        }
    }
}

/// Longest a forced-repaint nudge waits for the runtime to react before the
/// size is restored regardless. A runtime that writes nothing for this long is
/// dead or hung, and holding the shrunken size any longer helps nobody.
const REPAINT_NUDGE_TIMEOUT: Duration = Duration::from_millis(1500);
/// How often the nudge checks whether the runtime has written since the shrink.
const REPAINT_NUDGE_POLL: Duration = Duration::from_millis(20);

/// Waits until the runtime has written to its terminal since `quiet_since`,
/// and at least [`REPAINT_NUDGE_DELAY`] has passed, or [`REPAINT_NUDGE_TIMEOUT`]
/// runs out.
///
/// A fixed delay assumed the runtime notices the shrink promptly. On a loaded
/// machine a starved process can take far longer than that to handle
/// SIGWINCH, and a restore that lands first coalesces the two signals into one
/// wakeup that reads the original size and repaints nothing — the attached
/// terminal then stays blank until something else makes the runtime draw.
/// Output after the shrink is the proof that it woke up.
async fn wait_for_repaint(term: &TermBuffer, quiet_since: u64) {
    let started = tokio::time::Instant::now();
    tokio::time::sleep(REPAINT_NUDGE_DELAY).await;
    while term.latest_seq() == quiet_since && started.elapsed() < REPAINT_NUDGE_TIMEOUT {
        tokio::time::sleep(REPAINT_NUDGE_POLL).await;
    }
}
