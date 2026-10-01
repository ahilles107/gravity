//! Runtimes that raise permission requests themselves (Codex App Server
//! approvals): the supervisor relays them as internal events, and the answer
//! goes back through the bot's session.

use std::sync::Arc;

use tokio::sync::broadcast::error::RecvError;

use crate::app::AppState;
use crate::events::Internal;
use crate::runtime::PermissionAnswer;

use super::{decide, Answer, Decision};

/// Answers runtime permission requests for as long as the daemon runs.
pub async fn watch(app: Arc<AppState>) {
    let mut internal = app.events.subscribe_internal();
    loop {
        match internal.recv().await {
            Ok(Internal::RuntimePermission {
                bot_id,
                key,
                tool,
                input,
            }) => {
                let app = app.clone();
                tokio::spawn(async move {
                    let answer = match decide(&app, &bot_id, &tool, &input, Some(key)).await {
                        Some(Decision::Answered(Answer::AllowOnce, _)) => PermissionAnswer::Once,
                        Some(Decision::Answered(Answer::AllowSession, _)) => {
                            PermissionAnswer::Session
                        }
                        Some(Decision::Answered(Answer::Deny, _) | Decision::Expired) => {
                            PermissionAnswer::Deny
                        }
                        // No app could answer, or the runtime settled it: its
                        // own terminal has the prompt.
                        None => return,
                    };
                    if let Err(e) = app
                        .supervisor
                        .answer_runtime_permission(&bot_id, key, answer)
                    {
                        tracing::warn!(bot_id, error = %e, "could not hand the permission answer back");
                    }
                });
            }
            Ok(Internal::RuntimePermissionGone { bot_id, key }) => {
                app.approvals.withdraw(&bot_id, key);
            }
            Ok(_) => {}
            Err(RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "permission watcher lagged");
            }
            Err(RecvError::Closed) => break,
        }
    }
}
