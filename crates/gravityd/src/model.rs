//! Which model a bot talks with, and keeping that choice across restarts.
//!
//! Claude Code takes its model from the user-global settings unless the
//! session was spawned with `--model`, and `/model` inside a session is
//! scoped to that session. A bot therefore inherited whatever the global file
//! said at spawn time: pick Opus in a bot's terminal today, and the next
//! daemon restart quietly puts it back on whatever the global default has
//! become.
//!
//! Nothing reports the choice, but the session's JSONL transcript records the
//! model it is actually running — as a `model` attachment, and on every
//! assistant turn. Reading it back at start is what lets the daemon re-pin the
//! same model with `--model`, so a restart is invisible and a `/model` typed
//! into the terminal is the lasting choice.

use std::path::Path;

use serde_json::Value;

use crate::activity::{newest_transcript, tail_lines, transcript_dir, SCAN_LINES};

/// The model a turn ran with when Claude Code had no real one to name.
const SYNTHETIC: &str = "<synthetic>";

/// The model the workspace's newest session is running, or `None` when it has
/// no transcript yet.
///
/// The `model` attachment is authoritative because it carries the full id,
/// context-window suffix included; an assistant turn only names the base
/// model. Both are scanned newest-first, so whichever Claude Code wrote last
/// wins — a `/model` change lands in one of them straight away.
pub fn from_transcript(home: &Path, workspace: &Path) -> Option<String> {
    let path = newest_transcript(&transcript_dir(home, workspace))?;
    for line in tail_lines(&path, SCAN_LINES) {
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(model) = entry_model(&entry) {
            return Some(model);
        }
    }
    None
}

/// The model named by one transcript entry: the attachment's id, else the
/// model an assistant turn ran with.
fn entry_model(entry: &Value) -> Option<String> {
    let attachment = &entry["attachment"];
    if attachment["type"] == "model" {
        if let Some(id) = attachment["identity"]["modelId"].as_str() {
            return Some(id.to_string());
        }
    }
    let model = entry["message"]["model"].as_str()?;
    (model != SYNTHETIC && !model.is_empty()).then(|| model.to_string())
}

/// Whether `found` names the model already pinned as `pinned`.
///
/// A transcript names the model with or without its context-window suffix
/// (`claude-opus-5[1m]` on the attachment, `claude-opus-5` on the turn that
/// ran under it). Dropping the suffix is not a new choice, and treating it as
/// one would quietly re-pin the bot to the 200k variant of the same model.
pub fn is_same_choice(pinned: &str, found: &str) -> bool {
    pinned == found || pinned.split_once('[').map(|(base, _)| base) == Some(found)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn write_transcript(home: &Path, workspace: &Path, lines: &[Value]) {
        let dir = transcript_dir(home, workspace);
        fs::create_dir_all(&dir).expect("mkdir");
        let body: String = lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>();
        fs::write(dir.join("session.jsonl"), body).expect("write transcript");
    }

    fn attachment(model_id: &str) -> Value {
        serde_json::json!({
            "type": "attachment",
            "attachment": { "type": "model", "identity": { "modelId": model_id } }
        })
    }

    fn turn(model: &str) -> Value {
        serde_json::json!({ "type": "assistant", "message": { "model": model } })
    }

    #[test]
    fn reads_the_newest_model_the_session_named() {
        let home = tempfile::tempdir().expect("tempdir");
        let workspace = Path::new("/w/bots/alice/workspace");
        write_transcript(
            home.path(),
            workspace,
            &[
                attachment("claude-opus-5[1m]"),
                turn("claude-opus-5"),
                attachment("claude-fable-5-1"),
            ],
        );
        assert_eq!(
            from_transcript(home.path(), workspace).as_deref(),
            Some("claude-fable-5-1")
        );
    }

    #[test]
    fn falls_back_to_the_model_a_turn_ran_with() {
        let home = tempfile::tempdir().expect("tempdir");
        let workspace = Path::new("/w/bots/bob/workspace");
        write_transcript(
            home.path(),
            workspace,
            &[turn("claude-opus-5"), turn(SYNTHETIC)],
        );
        assert_eq!(
            from_transcript(home.path(), workspace).as_deref(),
            Some("claude-opus-5")
        );
    }

    #[test]
    fn returns_nothing_without_a_transcript() {
        let home = tempfile::tempdir().expect("tempdir");
        assert!(from_transcript(home.path(), Path::new("/w/absent/workspace")).is_none());
    }

    #[test]
    fn a_dropped_context_suffix_is_not_a_new_choice() {
        assert!(is_same_choice("claude-opus-5[1m]", "claude-opus-5"));
        assert!(is_same_choice("claude-opus-5", "claude-opus-5"));
        // The suffixed form carries more than the base one does, so arriving
        // at it *is* a change worth pinning.
        assert!(!is_same_choice("claude-opus-5", "claude-opus-5[1m]"));
        assert!(!is_same_choice("claude-opus-5[1m]", "claude-fable-5-1"));
    }
}
