//! The files a bot wrote, read from its transcript: each `Write`, `Edit`,
//! `MultiEdit` and `NotebookEdit` call, by path, keeping the first. This is
//! how the Files tab says which bot made a file.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;

/// How a bot first touched a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touch {
    Wrote,
    Edited,
}

/// Every path a bot wrote, with when and how it first did.
#[derive(Debug, Default)]
pub struct WriteLog {
    first: HashMap<String, (DateTime<Utc>, Touch)>,
}

impl WriteLog {
    /// Folds one transcript line in.
    pub fn push_line(&mut self, line: &str) {
        // Cheap filter first: most lines carry no file write.
        if !line.contains("\"file_path\"") && !line.contains("\"notebook_path\"") {
            return;
        }
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let Some(blocks) = record["message"]["content"].as_array() else {
            return;
        };
        let at = record["timestamp"]
            .as_str()
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map_or_else(Utc::now, |t| t.with_timezone(&Utc));
        for block in blocks.iter().filter(|b| b["type"] == "tool_use") {
            let touch = match block["name"].as_str() {
                Some("Write") => Touch::Wrote,
                Some("Edit" | "MultiEdit" | "NotebookEdit") => Touch::Edited,
                _ => continue,
            };
            let input = &block["input"];
            let Some(path) = input["file_path"]
                .as_str()
                .or_else(|| input["notebook_path"].as_str())
            else {
                continue;
            };
            self.first.entry(path.to_string()).or_insert((at, touch));
        }
    }

    /// When and how the bot first touched `path`.
    pub fn first(&self, path: &str) -> Option<(DateTime<Utc>, Touch)> {
        self.first.get(path).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_the_first_touch_of_each_path() {
        let mut log = WriteLog::default();
        let call = |name: &str, at: &str| {
            json!({"type": "assistant", "timestamp": at, "message": {"content": [
                {"type": "tool_use", "id": "x", "name": name, "input": {"file_path": "/a/report.md"}}]}})
            .to_string()
        };
        log.push_line(&call("Write", "2026-10-01T10:00:00Z"));
        log.push_line(&call("Edit", "2026-10-01T11:00:00Z"));
        let (at, touch) = log.first("/a/report.md").expect("written");
        assert_eq!(touch, Touch::Wrote);
        assert_eq!(at.to_rfc3339(), "2026-10-01T10:00:00+00:00");
        assert!(log.first("/a/other.md").is_none());
    }
}
