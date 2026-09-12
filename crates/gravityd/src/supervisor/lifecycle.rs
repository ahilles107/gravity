//! Session lifecycle: starting the runtime, consuming its event stream,
//! reconciling every live bot into a running session, and crash restart with
//! backoff.

use std::path::Path;

use super::*;

impl Supervisor {
    pub fn start_bot(&self, bot_id: &str) -> anyhow::Result<()> {
        let bot = self.inner.db.get_bot(bot_id)?.context("bot not found")?;

        {
            let bots = self.lock_bots();
            if let Some(h) = bots.get(bot_id) {
                if h.session.is_some() && h.state.is_running() {
                    return Ok(()); // already running
                }
            }
        }
        let token = self.inner.secrets.bot_token(bot_id)?;
        let workspace = std::path::PathBuf::from(&bot.workspace_path);
        let bot_root = workspace.parent().map(|p| p.to_path_buf());
        let artifacts = self
            .inner
            .db
            .get_project(&bot.project_id)?
            .map(|p| crate::paths::artifacts_dir(&self.inner.cfg, &p.dir_name));

        // Refresh the cooperative settings so existing bots pick up current
        // hooks (inbox-socket reporting, crossSessionInbound) on every start.
        if workspace.exists() {
            if let Err(e) =
                crate::paths::write_hook_settings(&workspace, self.inner.cfg.port, BOT_TOKEN_ENV)
            {
                tracing::warn!(bot_id, error = %e, "failed to refresh hook settings");
            }
            // Re-assert trust on every start, not just at creation: bots
            // provisioned before trust marking existed (or whose entry in
            // `~/.claude.json` was lost) would otherwise greet every daemon
            // restart with Claude Code's trust dialog.
            if self.inner.cfg.runtime == crate::config::RuntimeKind::Pty {
                if let Err(e) = crate::paths::trust_workspace(&self.inner.cfg.user_home, &workspace)
                {
                    tracing::warn!(bot_id, error = %e, "could not trust bot workspace");
                }
            }
            // Only fills in what is missing, so a bot provisioned before
            // `FACTS.md` existed gets one without losing what it has written.
            if let Err(e) = crate::paths::seed_memory_files(&workspace, &bot.name) {
                tracing::warn!(bot_id, error = %e, "failed to seed memory files");
            }
        }
        // Refresh `mcp.json` so a change to the daemon's port reaches existing
        // bots on their next start, just like the hook settings above. Written
        // once at creation otherwise, it would keep pointing at the old port.
        if let Some(root) = &bot_root {
            if let Err(e) = crate::paths::write_mcp_config(root, self.inner.cfg.port, BOT_TOKEN_ENV)
            {
                tracing::warn!(bot_id, error = %e, "failed to refresh mcp config");
            }
        }

        // A bot is one continuous conversation: every start after the first
        // resumes the workspace's session, so a daemon restart is invisible to
        // the bot and to whoever was talking to it.
        let resume = self.wants_resume(bot_id);
        let mut claude_args = self.inner.cfg.claude_args.clone();
        if resume {
            claude_args.push("--continue".to_string());
        }
        // Whatever model the bot was last talking with, it keeps talking with.
        let model = self.pinned_model(bot_id, &workspace);
        if let Some(model) = &model {
            claude_args.push("--model".to_string());
            claude_args.push(model.clone());
        }
        if let Some(root) = &bot_root {
            let mcp = root.join("mcp.json");
            if mcp.exists() {
                claude_args.push("--mcp-config".to_string());
                claude_args.push(mcp.display().to_string());
            }
            let system_md = root.join("system.md");
            if let Ok(content) = std::fs::read_to_string(&system_md) {
                claude_args.push("--append-system-prompt".to_string());
                claude_args.push(content);
            }
        }
        // The shared artifacts directory sits outside the workspace, so the
        // session needs it as an additional working directory, plus permission
        // rules granted here at spawn so the folder's own settings never have
        // to pre-approve anything.
        if let Some(artifacts) = &artifacts {
            claude_args.push("--add-dir".to_string());
            claude_args.push(artifacts.display().to_string());
            // One argument per rule: the flag is variadic, and joining them
            // would split a rule again on any comma in the artifacts path.
            claude_args.push("--allowedTools".to_string());
            claude_args.extend(crate::paths::artifacts_allow_rules(artifacts));
        }

        let mut env = vec![(BOT_TOKEN_ENV.to_string(), token)];
        if let Some(window) = self.inner.auto_compact.effective(&self.inner.cfg) {
            env.push((AUTO_COMPACT_WINDOW_ENV.to_string(), window.to_string()));
        }
        // Resume the full session unconditionally; see the consts for why the
        // summary chooser must never appear in an unattended pty.
        env.push((
            RESUME_THRESHOLD_MINUTES_ENV.to_string(),
            RESUME_THRESHOLD_MINUTES.to_string(),
        ));
        env.push((
            RESUME_TOKEN_THRESHOLD_ENV.to_string(),
            RESUME_TOKEN_THRESHOLD.to_string(),
        ));
        if self.inner.cfg.classic_renderer {
            env.push((DISABLE_ALTERNATE_SCREEN_ENV.to_string(), "1".to_string()));
            env.push((DISABLE_MOUSE_ENV.to_string(), "1".to_string()));
        }

        let spec = BotSpec {
            bot_id: bot.id.clone(),
            bot_name: bot.name.clone(),
            workspace,
            claude_bin: self.inner.cfg.claude_bin.clone(),
            claude_args,
            env,
            cols: 120,
            rows: 36,
        };

        let started = self.inner.adapter.start(&spec)?;
        // From here on the workspace has a conversation to come back to.
        if let Err(e) = self.inner.db.mark_bot_session(bot_id) {
            tracing::warn!(bot_id, error = %e, "could not record the bot's session");
        }
        let session = Arc::new(Mutex::new(started.session));
        let mut rx = started.events;
        let term = {
            let mut bots = self.lock_bots();
            let handle = bots
                .entry(bot_id.to_string())
                .or_insert_with(|| BotHandle::new(self.inner.cfg.scrollback_bytes));
            handle.session = Some(session.clone());
            handle.msg_socket = started.msg_socket;
            handle.size = (spec.cols, spec.rows);
            handle.stopping = false;
            handle.next_start_at = None;
            handle.resumed = resume;
            handle.pinned_model = model.is_some();
            // Consumed by this start: a fresh session leaves a transcript the
            // next start can resume again.
            handle.skip_resume = false;
            handle.state = BotState::Starting;
            handle.reason = "starting runtime".to_string();
            handle.last_start = Instant::now();
            handle.term.clone()
        };
        self.inner.events.push(Push::BotState {
            bot_id: bot_id.to_string(),
            state: BotState::Starting,
            reason: "starting runtime".to_string(),
            at: chrono::Utc::now().to_rfc3339(),
        });

        // Consume session events until exit.
        let sup = self.clone();
        let bot_id_owned = bot_id.to_string();
        tokio::spawn(async move {
            let mut saw_output = false;
            while let Some(ev) = rx.recv().await {
                match ev {
                    SessionEvent::Output(data) => {
                        term.push(data);
                        if !saw_output {
                            saw_output = true;
                            let (state, _) = sup.state(&bot_id_owned);
                            if state == BotState::Starting {
                                sup.set_state(&bot_id_owned, BotState::Ready, "runtime output");
                            }
                        }
                    }
                    SessionEvent::Exited { code } => {
                        sup.on_exit(&bot_id_owned, code);
                        break;
                    }
                }
            }
        });
        Ok(())
    }

    /// Bring every live bot into a running session. Called at boot and on
    /// every supervision tick, so a bot is running unless it is waiting out a
    /// crash backoff — there is nothing for a user to start.
    pub fn reconcile(&self) {
        let bots = match self.inner.db.list_bots(None) {
            Ok(bots) => bots,
            Err(e) => {
                tracing::warn!(error = %e, "reconcile could not list bots");
                return;
            }
        };
        for bot in bots {
            if !self.wants_start(&bot.id) {
                continue;
            }
            if let Err(e) = self.start_bot(&bot.id) {
                tracing::warn!(bot_id = %bot.id, error = %e, "autostart failed");
                // Back off like a crash so an unresolvable runtime does not
                // respawn on every tick.
                self.note_failed_start(&bot.id);
            }
        }
    }

    /// True when the next start should hand the runtime `--continue`: the bot
    /// has a conversation on disk and the last resume attempt did not fail
    /// immediately.
    fn wants_resume(&self, bot_id: &str) -> bool {
        let has_session = match self.inner.db.bot_has_session(bot_id) {
            Ok(has) => has,
            Err(e) => {
                tracing::warn!(bot_id, error = %e, "could not read the bot's session flag");
                false
            }
        };
        if !has_session {
            return false;
        }
        let bots = self.lock_bots();
        bots.get(bot_id).is_none_or(|h| !h.skip_resume)
    }

    /// The model the next session should be pinned to with `--model`, and the
    /// point where a `/model` typed into the terminal becomes the bot's
    /// lasting choice: the model its last session ran with is read back out of
    /// the transcript and stored, so the pin follows the user rather than the
    /// global settings file Claude Code would otherwise fall back to.
    ///
    /// `None` on a bot that has never run — its first session picks the
    /// default, and the start after that pins whatever it turned out to be.
    fn pinned_model(&self, bot_id: &str, workspace: &Path) -> Option<String> {
        let stored = match self.inner.db.bot_model(bot_id) {
            Ok(model) => model,
            Err(e) => {
                tracing::warn!(bot_id, error = %e, "could not read the bot's model");
                None
            }
        };
        // A bot that keeps crashing under a pin blames the pin: start without
        // one, and let the session that survives re-pin what it settles on.
        let condemned = self
            .lock_bots()
            .get(bot_id)
            .is_some_and(|h| h.pinned_model && h.consecutive_crashes >= MODEL_UNPIN_CRASHES);
        if condemned {
            tracing::warn!(bot_id, model = ?stored, "dropping the model pin after repeated crashes");
            self.store_model(bot_id, None);
            return None;
        }
        let found = crate::model::from_transcript(&self.inner.cfg.user_home, workspace);
        let unchanged = |found: &str| {
            stored
                .as_deref()
                .is_some_and(|pinned| crate::model::is_same_choice(pinned, found))
        };
        match found {
            Some(found) if !unchanged(&found) => {
                tracing::info!(bot_id, model = %found, "pinning the model the bot last ran with");
                self.store_model(bot_id, Some(&found));
                Some(found)
            }
            _ => stored,
        }
    }

    fn store_model(&self, bot_id: &str, model: Option<&str>) {
        if let Err(e) = self.inner.db.set_bot_model(bot_id, model) {
            tracing::warn!(bot_id, error = %e, "could not store the bot's model");
        }
    }

    /// True when the bot has no live session and its backoff, if any, is up.
    fn wants_start(&self, bot_id: &str) -> bool {
        let bots = self.lock_bots();
        match bots.get(bot_id) {
            None => true,
            Some(h) => {
                h.session.is_none()
                    && !h.stopping
                    && h.next_start_at.is_none_or(|at| Instant::now() >= at)
            }
        }
    }

    /// Record a failed start attempt and schedule the next one.
    fn note_failed_start(&self, bot_id: &str) {
        let mut bots = self.lock_bots();
        let handle = bots
            .entry(bot_id.to_string())
            .or_insert_with(|| BotHandle::new(self.inner.cfg.scrollback_bytes));
        handle.session = None;
        handle.consecutive_crashes += 1;
        handle.next_start_at = Some(Instant::now() + backoff(handle.consecutive_crashes));
    }

    fn on_exit(&self, bot_id: &str, code: Option<i32>) {
        let (was_stopping, crashes) = {
            let mut bots = self.lock_bots();
            let Some(h) = bots.get_mut(bot_id) else {
                return;
            };
            h.session = None;
            h.msg_socket = None;
            let was_stopping = h.stopping;
            if !was_stopping {
                // A resumed session that barely lived points at a transcript
                // the runtime choked on; come back fresh rather than replay
                // the same failure every restart.
                h.skip_resume = h.resumed && h.last_start.elapsed() < RESUME_FAST_EXIT;
                // Reset the crash counter when the session lived a while.
                if h.last_start.elapsed() > Duration::from_secs(300) {
                    h.consecutive_crashes = 0;
                }
                h.consecutive_crashes += 1;
                h.next_start_at = Some(Instant::now() + backoff(h.consecutive_crashes));
            } else {
                h.consecutive_crashes = 0;
                h.next_start_at = None;
            }
            (was_stopping, h.consecutive_crashes)
        };
        if was_stopping {
            self.set_state(bot_id, BotState::Stopped, "stopped by daemon");
            return;
        }
        let reason = format!("runtime exited with code {code:?}");
        self.set_state(bot_id, BotState::Crashed, &reason);
        // Only the first crash of a streak notifies: the reconciler retries
        // forever, and a bot that cannot start would otherwise toast on every
        // attempt.
        if crashes == 1 {
            self.inner.events.push(Push::notice(
                "error",
                "Bot crashed",
                format!("bot {bot_id}: {reason}"),
            ));
        }
    }
}

/// Restart delay for the nth consecutive crash: 2s doubling to a 5 minute cap.
fn backoff(crashes: u32) -> Duration {
    let shift = crashes.saturating_sub(1).min(8);
    (BASE_BACKOFF * 2u32.pow(shift)).min(MAX_BACKOFF)
}
