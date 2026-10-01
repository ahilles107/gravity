//! Turn building against records shaped like Claude Code 2.1's transcripts.

use std::collections::HashMap;

use serde_json::{json, Value};

use super::builder::Builder;
use super::model::{AsideKind, ChatItem, OwnerVia, StepStatus, Trigger};

fn build(records: &[Value]) -> Builder {
    let names = HashMap::from([("LEAD".to_string(), "lead".to_string())]);
    let mut builder = Builder::new("bot-1", names);
    let mut offset = 0u64;
    for record in records {
        let line = record.to_string();
        builder.push_line(offset, &line);
        offset += line.len() as u64 + 1;
    }
    builder
}

fn peer(uuid: &str, text: &str) -> Value {
    json!({
        "type": "user", "uuid": uuid, "timestamp": "2026-10-01T10:00:00Z",
        "isMeta": true, "origin": {"kind": "peer", "from": "unknown"},
        "promptSource": "system",
        "message": {"role": "user", "content": format!(
            "Another Claude session sent a message:\n{text}\n\nThis came from another \
             Claude session — not typed by your user."
        )}
    })
}

fn typed(uuid: &str, text: &str) -> Value {
    json!({
        "type": "user", "uuid": uuid, "timestamp": "2026-10-01T10:05:00Z",
        "origin": {"kind": "human"}, "promptSource": "typed",
        "message": {"role": "user", "content": text}
    })
}

fn assistant(uuid: &str, blocks: Value) -> Value {
    json!({
        "type": "assistant", "uuid": uuid, "timestamp": "2026-10-01T10:00:05Z",
        "message": {"role": "assistant", "content": blocks}
    })
}

fn result(uuid: &str, tool_use_id: &str, content: Value, extra: Value) -> Value {
    let mut record = json!({
        "type": "user", "uuid": uuid, "timestamp": "2026-10-01T10:00:06Z",
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": tool_use_id, "content": content}
        ]}
    });
    if let Value::Object(fields) = extra {
        for (key, value) in fields {
            record[key] = value;
        }
    }
    record
}

fn end_of_turn() -> Value {
    json!({"type": "system", "subtype": "turn_duration", "durationMs": 4896,
           "timestamp": "2026-10-01T10:01:00Z", "uuid": "end"})
}

#[test]
fn a_bus_task_becomes_a_turn_with_steps_and_a_result() {
    let builder = build(&[
        peer(
            "u1",
            "[msg #12 from LEAD · task · task_id t-1] Port the updater.",
        ),
        assistant(
            "a1",
            json!([
                {"type": "thinking", "thinking": "hmm"},
                {"type": "text", "text": "On it."},
                {"type": "tool_use", "id": "tool-1", "name": "Edit",
                 "input": {"file_path": "/w/src/update.rs", "old_string": "a", "new_string": "b"}}
            ]),
        ),
        result(
            "r1",
            "tool-1",
            json!("ok"),
            json!({"toolUseResult": {"structuredPatch": [
                {"oldStart": 1, "oldLines": 1, "newStart": 1, "newLines": 2,
                 "lines": ["-a", "+b", "+c"]}
            ]}}),
        ),
        assistant(
            "a2",
            json!([
                {"type": "tool_use", "id": "tool-2", "name": "mcp__gravity-bus__complete_task",
                 "input": {"task_id": "t-1", "result": "Ported.", "artifacts": ["/w/report.md"]}}
            ]),
        ),
        end_of_turn(),
    ]);
    assert_eq!(builder.turns.len(), 1);
    let turn = &builder.turns[0];
    assert!(!turn.open);
    assert_eq!(turn.duration_ms, Some(4896));
    assert_eq!(
        turn.trigger,
        Trigger::Bus {
            from: "lead".to_string(),
            msg_kind: "task".to_string(),
            num: 12,
            text: "Port the updater.".to_string(),
            task_id: Some("t-1".to_string()),
        }
    );
    assert!(matches!(&turn.items[0], ChatItem::Text { markdown, .. } if markdown == "On it."));
    match &turn.items[1] {
        ChatItem::Step(step) => {
            assert_eq!(step.title, "Edited update.rs");
            assert_eq!(step.status, StepStatus::Ok);
            assert_eq!((step.added, step.removed), (Some(2), Some(1)));
        }
        other => panic!("expected a step, got {other:?}"),
    }
    assert!(
        matches!(&turn.items[2], ChatItem::Completed { task_id, artifacts, .. }
        if task_id == "t-1" && artifacts[0].name == "report.md")
    );
    assert_eq!(
        (turn.stats.edits, turn.stats.added, turn.stats.removed),
        (1, 2, 1)
    );
    assert!(builder.steps.contains_key("tool-1"));
}

#[test]
fn the_owner_is_recognised_from_the_composer_and_the_terminal() {
    let builder = build(&[
        peer("u1", "[msg #3 from USER · chat] Which SDK are you on?"),
        assistant("a1", json!([{"type": "text", "text": "Windows 11 SDK."}])),
        end_of_turn(),
        typed(
            "u2",
            "<command-name>/compact</command-name>\n<command-args></command-args>",
        ),
    ]);
    assert_eq!(
        builder.turns[0].trigger,
        Trigger::Owner {
            text: "Which SDK are you on?".to_string(),
            via: OwnerVia::Chat
        }
    );
    assert_eq!(
        builder.turns[1].trigger,
        Trigger::Owner {
            text: "/compact".to_string(),
            via: OwnerVia::Terminal
        }
    );
    assert!(builder.turns[1].open);
}

#[test]
fn the_daemon_and_routines_are_not_the_owner() {
    let builder = build(&[
        peer(
            "u1",
            "[msg #1 from USER · note] You have just been created.",
        ),
        end_of_turn(),
        peer("u2", "[routine \"nightly\" #4 · run_id r-1] /report"),
    ]);
    assert!(matches!(&builder.turns[0].trigger, Trigger::Bus { from, .. } if from == "Gravity"));
    assert!(
        matches!(&builder.turns[1].trigger, Trigger::Routine { name, run_id, .. }
        if name == "nightly" && run_id.as_deref() == Some("r-1"))
    );
}

#[test]
fn images_failures_and_asides_are_kept() {
    let builder = build(&[
        typed("u1", "take a screenshot"),
        assistant(
            "a1",
            json!([
                {"type": "tool_use", "id": "shot", "name": "mcp__chrome__screenshot", "input": {}},
                {"type": "tool_use", "id": "bad", "name": "Bash", "input": {"command": "false"}}
            ]),
        ),
        result(
            "r1",
            "shot",
            json!([
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}},
                {"type": "text", "text": "Screenshot."}
            ]),
            json!({}),
        ),
        json!({"type": "user", "uuid": "r2", "timestamp": "2026-10-01T10:00:07Z",
               "message": {"content": [{"type": "tool_result", "tool_use_id": "bad",
                                        "is_error": true, "content": "exit 1"}]}}),
        json!({"type": "attachment", "uuid": "q1", "timestamp": "2026-10-01T10:00:08Z",
               "attachment": {"type": "queued_command",
                              "prompt": "[msg #9 from LEAD · note] FYI the build is green"}}),
        json!({"type": "user", "uuid": "i1", "timestamp": "2026-10-01T10:00:09Z",
               "message": {"content": [{"type": "text", "text": "[Request interrupted by user]"}]}}),
        json!({"type": "assistant", "uuid": "side", "isSidechain": true,
               "message": {"content": [{"type": "text", "text": "sub-agent chatter"}]}}),
    ]);
    let turn = &builder.turns[0];
    let ChatItem::Step(shot) = &turn.items[0] else {
        panic!("expected a step");
    };
    assert_eq!(shot.images.len(), 1);
    assert!(builder.images.contains_key(&shot.images[0].id));
    assert!(matches!(&turn.items[1], ChatItem::Step(s) if s.status == StepStatus::Error));
    assert!(
        matches!(&turn.items[2], ChatItem::Aside { kind: AsideKind::Incoming, text, .. }
        if text == "From lead · note: FYI the build is green")
    );
    assert!(matches!(
        &turn.items[3],
        ChatItem::Aside {
            kind: AsideKind::Interrupted,
            ..
        }
    ));
    assert_eq!(turn.items.len(), 4, "sidechain records are skipped");
    assert_eq!((turn.stats.images, turn.stats.errors), (1, 1));
}

#[test]
fn codex_observations_read_as_turns() {
    let builder = build(&[
        json!({"type": "user", "timestamp": "2026-10-01T10:00:00Z",
               "message": {"content": [{"type": "text", "text": "[msg #5 from USER · chat] hello"}]}}),
        json!({"type": "assistant", "timestamp": "2026-10-01T10:00:02Z",
               "message": {"content": [{"type": "text", "text": "Hi there."}]}}),
    ]);
    assert_eq!(builder.turns.len(), 1);
    assert!(matches!(
        &builder.turns[0].trigger,
        Trigger::Owner {
            via: OwnerVia::Chat,
            ..
        }
    ));
    assert!(
        builder.turns[0].id.starts_with('o'),
        "ids fall back to the line offset"
    );
}

#[test]
fn changed_turns_are_reported_once() {
    let mut builder = build(&[typed("u1", "hi")]);
    assert_eq!(builder.take_changed().len(), 1);
    assert!(builder.take_changed().is_empty());
    builder.push_line(
        999,
        &assistant("a1", json!([{"type": "text", "text": "hello"}])).to_string(),
    );
    assert_eq!(builder.take_changed()[0].id, "u1");
}
