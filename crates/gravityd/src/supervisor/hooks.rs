//! Runtime lifecycle hooks: what each Claude Code hook event means for a
//! bot's state.

use super::*;

impl Supervisor {
    /// Lifecycle hook received from the runtime (authenticated by bot token).
    /// `message` is the Notification hook's text, when the runtime sent one.
    pub fn on_hook(&self, bot_id: &str, event: &str, message: Option<&str>) {
        self.on_hook_with_transcript(bot_id, event, message, None);
    }

    pub fn on_hook_with_transcript(
        &self,
        bot_id: &str,
        event: &str,
        message: Option<&str>,
        transcript_path: Option<&str>,
    ) {
        match event {
            "SessionStart" => {
                // Socket registration happens in the HTTP handler. A session
                // can also start inside a process that is already running
                // (`/clear`), and the transition to ready on first output
                // fires only once per process, so mark it ready here — the
                // terminal is writable again as soon as the new session is up.
                if self.has_session(bot_id) {
                    self.set_state(bot_id, BotState::Ready, "session started");
                }
            }
            "UserPromptSubmit" => self.set_state(bot_id, BotState::Working, "prompt submitted"),
            // Answering a permission prompt fires no hook of its own, so a bot
            // that was approved and carried on used to sit on "waiting for
            // approval" until its turn ended. A completed tool call is the
            // proof the prompt is gone; other states are left alone, since a
            // tool can also run while the bot is already working.
            "PostToolUse" => {
                if self.state(bot_id).0 == BotState::WaitingForApproval {
                    self.set_state(bot_id, BotState::Working, "tool ran");
                }
            }
            "Stop" => {
                self.set_state(bot_id, BotState::Ready, "turn complete");
                self.inner.events.internal(Internal::BotDone {
                    bot_id: bot_id.to_string(),
                    transcript_path: transcript_path.map(str::to_string),
                });
            }
            "Notification" => self.on_notification(bot_id, message),
            // A session ending is not the runtime going away: `/clear` ends
            // one and opens the next inside the same process. Marking the bot
            // stopped there left a live terminal that the UI refused to type
            // into. A process that really exits is handled by `on_exit`.
            "SessionEnd" if self.has_session(bot_id) => {
                tracing::debug!(bot_id, "session ended, runtime still up")
            }
            "SessionEnd" => self.set_state(bot_id, BotState::Stopped, "session ended"),
            other => tracing::debug!(bot_id, event = other, "unhandled hook event"),
        }
    }

    /// Claude Code fires `Notification` both when it needs a permission
    /// decision and when the prompt has simply sat idle for a minute. Only the
    /// first is an approval: treating the idle one as an approval left a bot
    /// that had finished its turn stuck on "waiting for approval" forever.
    fn on_notification(&self, bot_id: &str, message: Option<&str>) {
        let text = message.unwrap_or_default();
        if is_idle_notification(text) {
            tracing::debug!(bot_id, message = text, "idle notification, state unchanged");
            return;
        }
        self.set_state(bot_id, BotState::WaitingForApproval, "notification");
        let detail = if text.is_empty() {
            "Claude is waiting for input or permission".to_string()
        } else {
            text.to_string()
        };
        self.inner.events.push(Push::ApprovalPending {
            bot_id: bot_id.to_string(),
            detail,
        });
    }
}

/// The "waiting for your input" notification means nobody is typing, not that
/// something needs approving. Everything else (permission prompts above all)
/// is treated as an approval stop.
fn is_idle_notification(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("waiting for your input") || m.contains("waiting for user input")
}
