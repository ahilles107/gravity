//! Manifests for the decision-registry tools.
//!
//! The descriptions carry more weight than usual. A bot that does not know the
//! registry exists asks in a terminal nobody is reading, and a bot that does
//! not know a settled ruling is authority re-opens a closed topic — both are
//! the failures this feature was built to end, and both are prevented here or
//! not at all.

use bus::MAX_OPEN_DECISIONS_PER_BOT;
use serde_json::{json, Value};

use super::schema::tool;

pub(super) fn decision_tools() -> Vec<Value> {
    vec![
        tool(
            "raise_decision",
            &format!(
                "Ask the owner for a ruling you cannot make yourself and no bot can give you. \
                  It goes to their Control center, not to a terminal, so it is seen whether or \
                  not they are watching you work. Say what happens under each option and what \
                  you will do while it is open, and always recommend one. \
                  Check list_decisions by tag first: a settled decision is the owner's \
                  authority and re-raising it wastes a ruling they already gave. \
                  You can have {MAX_OPEN_DECISIONS_PER_BOT} open at once. \
                  This does not block you — park the dependent work and carry on."
            ),
            json!({
                "title": {"type": "string", "description": "One line: what is being decided"},
                "body": {"type": "string", "description": "Context, evidence, what each option costs, and what you will do meanwhile"},
                "kind": {"type": "string", "enum": ["question", "decision"], "description": "'decision' offers options; 'question' expects free text"},
                "options": {
                    "type": "array",
                    "description": "Up to 8 choices",
                    "items": {"type": "object", "required": ["key", "label"], "properties": {
                        "key": {"type": "string", "description": "Short identifier the ruling names later"},
                        "label": {"type": "string"},
                        "description": {"type": "string", "description": "What happens if this is picked"}
                    }}
                },
                "recommendation": {"type": "string", "description": "The option key you recommend"},
                "tags": {"type": "array", "items": {"type": "string"}, "description": "Up to 8; reuse what list_tags shows"},
                "priority": {"type": "string", "enum": ["normal", "urgent"]},
                "deadline_at": {"type": "string", "description": "RFC 3339, when the choice stops being available"},
                "on_behalf_of": {"type": "string", "description": "Bot name whose work waits on this"},
                "source_task_id": {"type": "string"},
                "supersedes": {"type": "string", "description": "Decision id this replaces because the facts changed"}
            }),
            vec!["title", "body"],
        ),
        tool(
            "list_decisions",
            "Read the registry. Settled decisions are the owner's rulings and are \
              authority — check here before proposing something that may already be \
              settled, and lead with what changed if you think one no longer holds.",
            json!({
                "state": {"type": "string", "enum": ["open", "settled", "all"]},
                "tags": {"type": "array", "items": {"type": "string"}},
                "mine": {"type": "boolean", "description": "Only decisions you raised or wait on"},
                "query": {"type": "string", "description": "Words to match in the title, body or ruling; plain text, not a query language"},
                "limit": {"type": "integer"}
            }),
            vec![],
        ),
        tool(
            "get_decision",
            "The full record: body, options, the ruling, the thread, and who was told.",
            json!({ "id": {"type": "string"} }),
            vec!["id"],
        ),
        tool(
            "comment_decision",
            "Add to a decision's thread — new evidence, or an answer to what the owner \
              asked you there. This is where you answer an owner comment; do not reply on \
              the bus.",
            json!({
                "id": {"type": "string"},
                "body": {"type": "string"}
            }),
            vec!["id", "body"],
        ),
        tool(
            "withdraw_decision",
            "Withdraw a decision you raised that no longer needs an answer. Say why: the \
              owner may have already started drafting a ruling.",
            json!({
                "id": {"type": "string"},
                "reason": {"type": "string"}
            }),
            vec!["id", "reason"],
        ),
        tool(
            "record_decision",
            "File a ruling the owner gave you directly, in their own words, verbatim. \
              Use this whenever they decide something at your terminal, so the registry \
              stays complete and other bots stop re-asking. It is recorded as relayed by \
              you until the owner confirms it — which is honest, and is why another bot \
              should not treat it as authority the way it treats a published ruling. \
              Name any bot whose record this contradicts in 'notify'.",
            json!({
                "title": {"type": "string"},
                "body": {"type": "string", "description": "What was being decided and why"},
                "ruling_text": {"type": "string", "description": "The owner's words, verbatim"},
                "tags": {"type": "array", "items": {"type": "string"}},
                "notify": {"type": "array", "items": {"type": "string"}, "description": "Up to 16 bot names to tell — those whose record this contradicts"}
            }),
            vec!["title", "body", "ruling_text"],
        ),
        tool(
            "list_tags",
            "The shared tag taxonomy, with how often each is used in your project. \
              Reuse an existing tag rather than coining a synonym.",
            json!({}),
            vec![],
        ),
        tool(
            "upsert_tag",
            "Create a tag or describe an existing one. The description is what stops \
              one bot filing 'spend' and another filing 'budget' for the same thing.",
            json!({
                "name": {"type": "string", "description": "1-32 characters of a-z, 0-9 and '-'"},
                "description": {"type": "string"},
                "color": {"type": "string", "description": "'#rgb' or '#rrggbb'"}
            }),
            vec!["name"],
        ),
        tool(
            "retire_tag",
            "Hide a tag from pickers, optionally merging everything it holds into \
              another. Existing links are kept, so nothing is unfiled.",
            json!({
                "name": {"type": "string", "description": "1-32 characters of a-z, 0-9 and '-'"},
                "into": {"type": "string", "description": "An existing tag to merge into"}
            }),
            vec!["name"],
        ),
    ]
}
