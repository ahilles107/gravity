//! The Codex bot's conversation record, `codex-observations.jsonl`, written in
//! the shape of Claude Code's transcript so the chat reads both the same way:
//! user and assistant text, each tool item as a `tool_use` with its
//! `tool_result`, and a `turn_duration` record where a turn ends.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use serde_json::{json, Value};

fn write(path: &Path, record: &Value) -> anyhow::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{record}")?;
    Ok(())
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub(super) fn append(path: &Path, role: &str, text: &str) -> anyhow::Result<()> {
    write(
        path,
        &json!({ "type": role, "timestamp": now(), "message": { "content": [{ "type": "text", "text": text }] } }),
    )
}

/// Marks the end of a turn, as Claude Code's `turn_duration` does.
pub(super) fn turn_end(path: &Path, duration_ms: Option<u128>) -> anyhow::Result<()> {
    write(
        path,
        &json!({ "type": "system", "subtype": "turn_duration", "timestamp": now(), "durationMs": duration_ms }),
    )
}

/// One tool call as it appears in the chat.
struct Call {
    id: String,
    name: String,
    input: Value,
    output: String,
    failed: bool,
    patch: Option<Vec<Value>>,
}

/// Records a completed App Server item if it is a tool call; text items are
/// recorded by the worker itself.
pub(super) fn tool_item(path: &Path, item: &Value) -> anyhow::Result<()> {
    for call in calls(item) {
        write(
            path,
            &json!({
                "type": "assistant", "timestamp": now(),
                "message": { "content": [{ "type": "tool_use", "id": call.id, "name": call.name, "input": call.input }] }
            }),
        )?;
        let mut result = json!({
            "type": "user", "timestamp": now(),
            "message": { "content": [{
                "type": "tool_result", "tool_use_id": call.id,
                "content": call.output, "is_error": call.failed
            }] }
        });
        if let Some(patch) = call.patch {
            result["toolUseResult"] = json!({ "structuredPatch": patch });
        }
        write(path, &result)?;
    }
    Ok(())
}

fn failed_status(item: &Value) -> bool {
    matches!(item["status"].as_str(), Some("failed" | "declined"))
}

fn calls(item: &Value) -> Vec<Call> {
    let id = item["id"].as_str().unwrap_or_default().to_string();
    match item["type"].as_str() {
        Some("commandExecution") => vec![Call {
            id,
            name: "Bash".to_string(),
            input: json!({ "command": item["command"], "cwd": item["cwd"] }),
            output: item["aggregatedOutput"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            failed: failed_status(item) || item["exitCode"].as_i64().is_some_and(|code| code != 0),
            patch: None,
        }],
        Some("fileChange") => item["changes"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, change)| {
                let kind = change["kind"]["type"].as_str().unwrap_or("update");
                Call {
                    id: format!("{id}:{index}"),
                    name: if kind == "add" { "Write" } else { "Edit" }.to_string(),
                    input: json!({ "file_path": change["path"] }),
                    output: item["status"].as_str().unwrap_or("completed").to_string(),
                    failed: failed_status(item),
                    patch: Some(hunks(change["diff"].as_str().unwrap_or_default(), kind)),
                }
            })
            .collect(),
        Some("mcpToolCall") => vec![Call {
            id,
            name: format!(
                "mcp__{}__{}",
                item["server"].as_str().unwrap_or("mcp"),
                item["tool"].as_str().unwrap_or("tool")
            ),
            input: item["arguments"].clone(),
            output: match item["error"]["message"].as_str() {
                Some(error) => error.to_string(),
                None => texts(&item["result"]["content"]),
            },
            failed: !item["error"].is_null() || failed_status(item),
            patch: None,
        }],
        Some("webSearch") => vec![Call {
            id,
            name: "WebSearch".to_string(),
            input: json!({ "query": item["query"] }),
            output: String::new(),
            failed: false,
            patch: None,
        }],
        _ => Vec::new(),
    }
}

/// The text parts of an MCP result's content.
fn texts(content: &Value) -> String {
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A unified diff as Claude Code's structured patch hunks. A new or deleted
/// file whose diff has no hunk header is one hunk of added or removed lines.
pub(super) fn hunks(diff: &str, kind: &str) -> Vec<Value> {
    if !diff.lines().any(|line| line.starts_with("@@ ")) {
        let sign = match kind {
            "add" => '+',
            "delete" => '-',
            _ => return Vec::new(),
        };
        let lines: Vec<String> = diff.lines().map(|line| format!("{sign}{line}")).collect();
        let count = lines.len();
        let (old_lines, new_lines) = if sign == '+' { (0, count) } else { (count, 0) };
        return vec![json!({
            "oldStart": 0, "oldLines": old_lines, "newStart": 1, "newLines": new_lines, "lines": lines
        })];
    }
    let mut out: Vec<Value> = Vec::new();
    for line in diff.lines() {
        if let Some(header) = line.strip_prefix("@@ ") {
            out.push(hunk_header(header));
        } else if line.starts_with("--- ") || line.starts_with("+++ ") || line.starts_with("diff ")
        {
            continue;
        } else if let Some(lines) = out.last_mut().and_then(|h| h["lines"].as_array_mut()) {
            lines.push(json!(line));
        }
    }
    out
}

/// `-a,b +c,d @@ …` → a hunk with no lines yet.
fn hunk_header(header: &str) -> Value {
    let mut numbers = header.split_whitespace().take(2).map(|range| {
        let range = range.trim_start_matches(['-', '+']);
        let (start, count) = range.split_once(',').unwrap_or((range, "1"));
        (
            start.parse::<i64>().unwrap_or(0),
            count.parse::<i64>().unwrap_or(1),
        )
    });
    let (old_start, old_lines) = numbers.next().unwrap_or((0, 0));
    let (new_start, new_lines) = numbers.next().unwrap_or((0, 0));
    json!({ "oldStart": old_start, "oldLines": old_lines, "newStart": new_start, "newLines": new_lines, "lines": [] })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_unified_diff_as_hunks() {
        let diff = "--- a/x.rs\n+++ b/x.rs\n@@ -1,2 +1,3 @@\n fn a() {}\n-fn b() {}\n+fn b() { 1 }\n+fn c() {}\n";
        let hunks = hunks(diff, "update");
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0]["oldStart"], 1);
        assert_eq!(hunks[0]["newLines"], 3);
        assert_eq!(hunks[0]["lines"].as_array().map(Vec::len), Some(4));
    }

    #[test]
    fn a_new_file_without_headers_is_all_additions() {
        let hunks = hunks("line one\nline two", "add");
        assert_eq!(hunks[0]["lines"], json!(["+line one", "+line two"]));
    }

    #[test]
    fn writes_tool_items_in_claude_codes_shape() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("obs.jsonl");
        tool_item(
            &path,
            &json!({"type": "commandExecution", "id": "c1", "command": "cargo test",
                    "cwd": "/w", "aggregatedOutput": "ok", "exitCode": 1, "status": "completed"}),
        )
        .expect("write");
        tool_item(
            &path,
            &json!({"type": "agentMessage", "id": "m", "text": "hi"}),
        )
        .expect("skip");
        let text = std::fs::read_to_string(&path).expect("read");
        let records: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).expect("json"))
            .collect();
        assert_eq!(records.len(), 2, "text items are not tool calls");
        assert_eq!(records[0]["message"]["content"][0]["name"], "Bash");
        assert_eq!(records[1]["message"]["content"][0]["tool_use_id"], "c1");
        assert_eq!(records[1]["message"]["content"][0]["is_error"], true);
    }
}
