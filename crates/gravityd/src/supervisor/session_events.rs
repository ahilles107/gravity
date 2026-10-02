//! A running session's event stream: terminal output, lifecycle hooks, exit,
//! and permission requests the runtime raises for the owner.

use std::sync::Arc;

use bus::BotState;
use tokio::sync::mpsc;

use crate::events::Internal;
use crate::runtime::{PermissionAnswer, SessionEvent};
use crate::terminal::TermBuffer;

use super::Supervisor;

/// Consumes one session's events until it exits.
pub(super) async fn consume(
    sup: Supervisor,
    bot_id: String,
    term: Arc<TermBuffer>,
    mut rx: mpsc::UnboundedReceiver<SessionEvent>,
) {
    let mut saw_output = false;
    while let Some(event) = rx.recv().await {
        match event {
            SessionEvent::Output(data) => {
                term.push(data);
                if !saw_output {
                    saw_output = true;
                    if sup.state(&bot_id).0 == BotState::Starting {
                        sup.set_state(&bot_id, BotState::Ready, "runtime output");
                    }
                }
            }
            SessionEvent::Exited { code } => {
                sup.on_exit(&bot_id, code);
                break;
            }
            SessionEvent::Lifecycle {
                event,
                detail,
                transcript,
            } => {
                sup.on_hook_with_transcript(
                    &bot_id,
                    event,
                    detail.as_deref(),
                    transcript.as_deref(),
                );
            }
            // Answered by the approval broker, which owns the owner's cards.
            SessionEvent::Permission { key, tool, input } => {
                sup.inner.events.internal(Internal::RuntimePermission {
                    bot_id: bot_id.clone(),
                    key,
                    tool,
                    input,
                });
            }
            SessionEvent::PermissionGone { key } => {
                sup.inner.events.internal(Internal::RuntimePermissionGone {
                    bot_id: bot_id.clone(),
                    key,
                });
            }
        }
    }
}

impl Supervisor {
    /// Hands the owner's answer to the session that asked.
    pub fn answer_runtime_permission(
        &self,
        bot_id: &str,
        key: u64,
        answer: PermissionAnswer,
    ) -> anyhow::Result<()> {
        let session = self
            .lock_bots()
            .get(bot_id)
            .and_then(|h| h.session.clone())
            .ok_or_else(|| anyhow::anyhow!("bot has no running session"))?;
        let mut session = session.lock().unwrap_or_else(|e| e.into_inner());
        session.answer_permission(key, answer)
    }
}
