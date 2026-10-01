//! What woke a bot: the trigger a user record in the transcript opens a turn
//! with, read from Claude Code's record markers and Gravity's envelopes.

use std::collections::HashMap;

use serde_json::Value;

use super::envelope::{self, Delivered};
use super::model::{OwnerVia, Trigger};
use super::steps;

/// Longest trigger text kept in a turn.
pub(super) const MAX_TRIGGER_CHARS: usize = 4_000;

/// The trigger a user record starts a turn with, or `None` for runtime
/// bookkeeping (compaction summaries, local command output, meta records).
pub(super) fn of_prompt(
    record: &Value,
    text: &str,
    names: &HashMap<String, String>,
) -> Option<Trigger> {
    let flag = |key: &str| record.get(key).and_then(Value::as_bool) == Some(true);
    if flag("isCompactSummary") || flag("isVisibleInTranscriptOnly") {
        return None;
    }
    let origin = record["origin"]["kind"].as_str();
    let trigger = if origin == Some("task-notification") || text.starts_with("<task-notification>")
    {
        Trigger::Background {
            text: tag(text, "summary")
                .unwrap_or("A background task finished")
                .to_string(),
        }
    } else if record.get("scheduledTaskId").is_some() {
        Trigger::Background {
            text: format!("Scheduled wake-up: {}", first_line(text)),
        }
    } else if let Some(delivered) = envelope::parse(text) {
        delivered_trigger(names, delivered)
    } else if origin == Some("human") {
        Trigger::Owner {
            text: owner_text(text),
            via: OwnerVia::Terminal,
        }
    } else if text.starts_with("<local-command") || flag("isMeta") {
        return None;
    } else {
        Trigger::Owner {
            text: owner_text(text),
            via: OwnerVia::Terminal,
        }
    };
    Some(trigger)
}

fn delivered_trigger(names: &HashMap<String, String>, delivered: Delivered) -> Trigger {
    match delivered {
        Delivered::Message {
            num,
            from,
            kind,
            task_id,
            body,
        } => {
            let body = steps::truncate(&body, MAX_TRIGGER_CHARS);
            if from == "USER" && kind == "chat" {
                Trigger::Owner {
                    text: body,
                    via: OwnerVia::Chat,
                }
            } else {
                // `USER` on anything but a chat is the daemon speaking:
                // introductions, expiries, renames.
                let from = if from == "USER" {
                    "Gravity".to_string()
                } else {
                    names.get(&from).cloned().unwrap_or(from)
                };
                Trigger::Bus {
                    from,
                    msg_kind: kind,
                    num,
                    text: body,
                    task_id,
                }
            }
        }
        Delivered::Routine {
            name,
            run_id,
            prompt,
        } => Trigger::Routine {
            name,
            text: steps::truncate(&prompt, MAX_TRIGGER_CHARS),
            run_id,
        },
        Delivered::Decision { decision_id, text } => Trigger::Ruling {
            decision_id,
            text: steps::truncate(&text, MAX_TRIGGER_CHARS),
        },
    }
}

/// A slash command typed into the terminal reads as the command, not markup.
fn owner_text(text: &str) -> String {
    match tag(text, "command-name") {
        Some(command) => match tag(text, "command-args").filter(|a| !a.is_empty()) {
            Some(args) => format!("{command} {args}"),
            None => command.to_string(),
        },
        None => steps::truncate(text, MAX_TRIGGER_CHARS),
    }
}

fn tag<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].trim())
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}
