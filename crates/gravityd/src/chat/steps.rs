//! How a tool call reads in the chat: a human title for the step, or, for
//! Gravity's own bus tools, the message, result or decision it really was.

use serde_json::Value;

use super::model::FileRef;

/// The MCP server name Gravity registers its bus under.
const BUS_PREFIX: &str = "mcp__gravity-bus__";

/// Bus tools that are housekeeping, not something the owner wants to read.
const MINOR_BUS_TOOLS: &[&str] = &[
    "check_inbox",
    "list_bots",
    "get_self",
    "list_routines",
    "list_decisions",
    "get_decision",
    "list_tags",
];

/// Built-in tools that are plumbing rather than work.
const MINOR_TOOLS: &[&str] = &[
    "ToolSearch",
    "Skill",
    "TaskStop",
    "TaskOutput",
    "TodoWrite",
    "ScheduleWakeup",
    "Monitor",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Command,
    Read,
    Edit,
    Other,
}

#[derive(Debug, PartialEq)]
pub struct Described {
    pub title: String,
    pub subtitle: Option<String>,
    pub minor: bool,
    pub kind: ToolKind,
}

/// A bus tool call, read as what it did on the bus.
#[derive(Debug, PartialEq)]
pub enum BusCall {
    Sent {
        to: String,
        kind: String,
        body: String,
    },
    Completed {
        task_id: String,
        result: String,
        artifacts: Vec<FileRef>,
    },
    Decision {
        title: String,
    },
}

pub fn bus_call(name: &str, input: &Value) -> Option<BusCall> {
    let tool = name.strip_prefix(BUS_PREFIX)?;
    let text = |key: &str| {
        input
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    match tool {
        "send_message" => Some(BusCall::Sent {
            to: text("to"),
            kind: input
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("note")
                .to_string(),
            body: text("body"),
        }),
        "complete_task" => Some(BusCall::Completed {
            task_id: text("task_id"),
            result: text("result"),
            artifacts: input
                .get("artifacts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(file_ref)
                .collect(),
        }),
        "raise_decision" => Some(BusCall::Decision {
            title: text("title"),
        }),
        _ => None,
    }
}

pub fn file_ref(path: &str) -> FileRef {
    FileRef {
        path: path.to_string(),
        name: base_name(path).to_string(),
    }
}

pub fn describe(name: &str, input: &Value) -> Described {
    let field = |key: &str| input.get(key).and_then(Value::as_str);
    let path = field("file_path").or_else(|| field("notebook_path"));
    let file = path.map(base_name).unwrap_or("a file");
    let (title, subtitle, kind) = match name {
        "Bash" => (
            field("description")
                .map(str::to_string)
                .unwrap_or_else(|| "Ran a command".to_string()),
            field("command").map(first_line),
            ToolKind::Command,
        ),
        "Read" => (
            format!("Read {file}"),
            path.map(str::to_string),
            ToolKind::Read,
        ),
        "Edit" | "MultiEdit" | "NotebookEdit" => (
            format!("Edited {file}"),
            path.map(str::to_string),
            ToolKind::Edit,
        ),
        "Write" => (
            format!("Wrote {file}"),
            path.map(str::to_string),
            ToolKind::Edit,
        ),
        "Grep" | "Glob" => (
            format!("Searched for {}", field("pattern").unwrap_or("files")),
            field("path").map(str::to_string),
            ToolKind::Read,
        ),
        "WebFetch" => (
            format!("Fetched {}", field("url").map(host).unwrap_or("a page")),
            field("url").map(str::to_string),
            ToolKind::Other,
        ),
        "WebSearch" => (
            format!(
                "Searched the web for {}",
                field("query").unwrap_or("something")
            ),
            None,
            ToolKind::Other,
        ),
        "Task" | "Agent" => (
            format!(
                "Ran a sub-agent: {}",
                field("description").unwrap_or("a task")
            ),
            None,
            ToolKind::Other,
        ),
        "TodoWrite" => ("Updated the plan".to_string(), None, ToolKind::Other),
        other => match other.strip_prefix("mcp__") {
            Some(rest) => {
                let (server, tool) = rest.split_once("__").unwrap_or(("mcp", rest));
                (
                    tool.replace('_', " "),
                    Some(server.to_string()),
                    ToolKind::Other,
                )
            }
            None => (other.to_string(), None, ToolKind::Other),
        },
    };
    let minor = MINOR_TOOLS.contains(&name)
        || name
            .strip_prefix(BUS_PREFIX)
            .is_some_and(|tool| MINOR_BUS_TOOLS.contains(&tool));
    Described {
        title,
        subtitle: subtitle.map(|s| truncate(&s, 160)),
        minor,
        kind,
    }
}

/// The last component of a path written on either platform.
pub fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(path)
}

fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split('/').next().unwrap_or(rest)
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().to_string()
}

/// At most `max` characters, with an ellipsis when cut.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

/// The last `max` characters, for output whose end is what matters.
pub fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let mut out = String::from("…");
    out.extend(text.chars().skip(count - max));
    out
}

/// Lines added and removed by an edit, from Claude Code's structured patch.
pub fn patch_counts(result: &Value) -> (u32, u32) {
    let mut added = 0;
    let mut removed = 0;
    for hunk in result["structuredPatch"].as_array().into_iter().flatten() {
        for line in hunk["lines"].as_array().into_iter().flatten() {
            match line.as_str().and_then(|l| l.chars().next()) {
                Some('+') => added += 1,
                Some('-') => removed += 1,
                _ => {}
            }
        }
    }
    (added, removed)
}

/// A tool result's text, whether it is a string or text blocks.
pub fn result_text(block: &Value) -> Option<String> {
    match &block["content"] {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => Some(
            parts
                .iter()
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn titles_common_tools() {
        let bash = describe(
            "Bash",
            &json!({"command": "cargo test\nmore", "description": "Run tests"}),
        );
        assert_eq!(bash.title, "Run tests");
        assert_eq!(bash.subtitle.as_deref(), Some("cargo test"));
        assert_eq!(bash.kind, ToolKind::Command);
        let edit = describe("Edit", &json!({"file_path": "C:\\work\\src\\app.ts"}));
        assert_eq!(edit.title, "Edited app.ts");
        assert!(describe("mcp__gravity-bus__check_inbox", &json!({})).minor);
        assert_eq!(
            describe("mcp__chrome__take_screenshot", &json!({})).title,
            "take screenshot"
        );
    }

    #[test]
    fn reads_bus_tools_as_what_they_did() {
        let call = bus_call(
            "mcp__gravity-bus__complete_task",
            &json!({"task_id": "t", "result": "done", "artifacts": ["/a/b/report.md"]}),
        );
        assert_eq!(
            call,
            Some(BusCall::Completed {
                task_id: "t".to_string(),
                result: "done".to_string(),
                artifacts: vec![FileRef {
                    path: "/a/b/report.md".to_string(),
                    name: "report.md".to_string()
                }],
            })
        );
        assert_eq!(bus_call("Bash", &json!({})), None);
    }
}
