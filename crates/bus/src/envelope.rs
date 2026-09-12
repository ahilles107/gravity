//! Text rendering of bus envelopes at the runtime boundary.
//!
//! Envelopes stay structured everywhere inside the daemon; only the runtime
//! adapter renders them to the text injected into a Claude session.

mod decisions;

pub use decisions::{render_decision, DecisionPhase};

use crate::types::{Message, MessageKind, SenderKind};

/// Render a message for injection into a bot's session, e.g.
/// `[msg #42 from BOB · task] Please review PR #1121.`
///
/// A delegated task carries its own id in the header. It is the only place the
/// assignee ever sees it, and without it `complete_task` cannot be called — the
/// requester would wait on a result the assignee has no way to publish.
pub fn render_message(msg: &Message, ref_num: Option<i64>, task_id: Option<&str>) -> String {
    let from = match msg.sender.kind {
        SenderKind::User => "USER".to_string(),
        SenderKind::Bot | SenderKind::Routine => msg.sender.name.to_uppercase(),
    };
    let mut head = format!("[msg #{} from {} · {}", msg.num, from, msg.kind.as_str());
    if let Some(n) = ref_num {
        head.push_str(&format!(" · re #{n}"));
    }
    if let Some(id) = task_id {
        head.push_str(&format!(" · task_id {id}"));
    }
    head.push(']');
    format!("{head} {}", sanitize_body(&msg.body))
}

/// Render a routine occurrence, e.g.
/// `[routine "weekly-report" #7 · run_id <uuid>] /weekly-report`.
pub fn render_routine(
    routine_name: &str,
    run_num: i64,
    run_id: Option<&str>,
    prompt: &str,
) -> String {
    let name = sanitize_header_field(routine_name);
    let run = run_id
        .map(|id| format!(" · run_id {id}"))
        .unwrap_or_default();
    format!(
        "[routine \"{}\" #{}{}] {}",
        name,
        run_num,
        run,
        sanitize_body(prompt)
    )
}

pub(super) fn sanitize_header_field(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '[' => '(',
            ']' => ')',
            '"' => '\'',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect()
}

/// Inbound bodies are untrusted prompt input: strip control characters that
/// could fake terminal UI or split the envelope header from its body.
pub(super) fn sanitize_body(body: &str) -> String {
    body.chars()
        .map(|c| {
            if c == '\n' || c == '\t' {
                c
            } else if c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// True when the message kind expects a correlated reply.
pub fn expects_reply(kind: MessageKind) -> bool {
    matches!(kind, MessageKind::Task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{new_id, now, Sender, SenderKind};

    fn msg(kind: MessageKind, body: &str) -> Message {
        Message {
            id: new_id(),
            num: 42,
            conversation_id: new_id(),
            sender: Sender {
                kind: SenderKind::Bot,
                bot_id: Some(new_id()),
                name: "bob".to_string(),
            },
            kind,
            body: body.to_string(),
            ref_message_id: None,
            decision_id: None,
            created_at: now(),
        }
    }

    #[test]
    fn renders_task_envelope() {
        let m = msg(MessageKind::Task, "Please review PR #1121.");
        assert_eq!(
            render_message(&m, None, None),
            "[msg #42 from BOB · task] Please review PR #1121."
        );
    }

    #[test]
    fn renders_reply_reference() {
        let m = msg(MessageKind::Done, "Reviewed.");
        assert_eq!(
            render_message(&m, Some(7), None),
            "[msg #42 from BOB · done · re #7] Reviewed."
        );
    }

    #[test]
    fn strips_control_characters() {
        let m = msg(MessageKind::Note, "a\u{1b}[31mred\u{7}b");
        assert_eq!(
            render_message(&m, None, None),
            "[msg #42 from BOB · note] a [31mred b"
        );
    }

    #[test]
    fn renders_task_id_for_the_assignee() {
        let m = msg(MessageKind::Task, "Review it.");
        assert_eq!(
            render_message(&m, None, Some("t-1")),
            "[msg #42 from BOB · task · task_id t-1] Review it."
        );
    }

    #[test]
    fn renders_routine_envelope() {
        assert_eq!(
            render_routine("weekly-report", 7, None, "/weekly-report"),
            "[routine \"weekly-report\" #7] /weekly-report"
        );
    }

    #[test]
    fn renders_the_correlated_run_id() {
        assert_eq!(
            render_routine("weekly-report", 7, Some("run-1"), "/weekly-report"),
            "[routine \"weekly-report\" #7 · run_id run-1] /weekly-report"
        );
    }

    #[test]
    fn routine_name_cannot_break_the_envelope_header() {
        assert_eq!(
            render_routine("bad\"]\nname", 7, Some("run-1"), "/weekly-report"),
            "[routine \"bad') name\" #7 · run_id run-1] /weekly-report"
        );
    }
}
