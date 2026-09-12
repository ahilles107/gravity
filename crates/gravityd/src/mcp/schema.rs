//! The MCP tool manifest: every tool a bot can call, and its schema.

use serde_json::{json, Value};

/// One tool manifest entry. Shared with the decision tools, which live in
/// their own module so neither list has to be scrolled past to read the other.
pub(super) fn tool(name: &str, desc: &str, props: Value, required: Vec<&str>) -> Value {
    json!({
        "name": name,
        "description": desc,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": required
        }
    })
}

pub(super) fn tool_list() -> Value {
    let mut tools = core_tools();
    tools.extend(super::schema_decisions::decision_tools());
    json!({ "tools": tools })
}

fn core_tools() -> Vec<Value> {
    vec![
        tool("send_message",
             "Send a message to another bot in your project. kind: task|reply|note. \
              The recipient acts on it directly — a 'task' is a work order, not a suggestion — \
              so state plainly what you want done and any constraints on it. \
              A task expires after 24h unless completed, and its replies are capped, \
              so budget the exchange. \
              To report the result of a task delegated to you, use complete_task instead; \
              kind 'task' back to a bot that delegated to you is refused as a loop.",
             json!({
                 "to": {"type": "string", "description": "Bot name"},
                 "body": {"type": "string"},
                 "kind": {"type": "string", "enum": ["task", "reply", "note"]},
                 "ref": {"type": "string", "description": "Message id this replies to"},
                 "deadline_hours": {"type": "integer", "description": "Deadline for a kind 'task' send, in hours (default 24, max 168)"}
             }),
             vec!["to", "body"]),
        tool("complete_task",
             "Mark a delegated task done and publish the result to the requester. \
              The task_id is in the message envelope (`· task_id <id>`) and in check_inbox. \
              List files you produced in artifacts so the requester reads them directly.",
             json!({
                 "task_id": {"type": "string"},
                 "result": {"type": "string"},
                 "artifacts": {"type": "array", "items": {"type": "string"}}
             }),
             vec!["task_id", "result"]),
        tool("cancel_task",
             "Close an open task you delegated, when the bot holding it will never \
              finish it or the work is no longer wanted. Only the bot that delegated \
              the task can cancel it; the assignee is told to stop, and the fan-out \
              slot the task held is freed. check_inbox lists the tasks you have \
              delegated and their ids.",
             json!({
                 "task_id": {"type": "string"},
                 "reason": {"type": "string", "description": "Why it is being cancelled, passed on to the assignee. Max 64KB."}
             }),
             vec!["task_id"]),
        tool("list_bots",
             "List bots in your project with status. The id is what a signal \
              trigger's from_bot_id refers to.",
             json!({}), vec![]),
        tool("check_inbox",
             "Fetch and acknowledge your unread messages. A message that delegated a task \
              to you carries its task_id. delegated_tasks lists the tasks you are still \
              waiting on, with the ids complete_task and cancel_task take. \
              open_decisions lists what you have asked the owner and not yet heard back on.",
             json!({}), vec![]),
        tool("create_routine",
             "Schedule a routine for yourself. It is enabled and starts running immediately.",
             json!({
                 "name": {"type": "string"},
                 "trigger": {"type": "object", "description": "{kind:'cron',expr,tz} | {kind:'interval',seconds} | {kind:'signal',name,from_bot_id?}. A signal trigger waits for emit_signal to announce that name; from_bot_id is a bot id from list_bots, and omitting it reacts to any bot in your project."},
                 "prompt": {"type": "string"},
                 "busy_policy": {"type": "string", "enum": ["skip", "queue_one", "queue_all", "replace"]},
                 "max_duration_seconds": {"type": "integer", "description": "Give up on one run after this long. Defaults to the daemon's scheduler lease."},
                 "max_attempts": {"type": "integer", "description": "Attempts per occurrence, including the first. 1 (the default) never retries."},
             }),
             vec!["name", "trigger", "prompt"]),
        tool("list_routines", "List your routines.", json!({}), vec![]),
        tool("set_routine_enabled",
             "Enable or disable one of your routines.",
             json!({
                 "routine_id": {"type": "string"},
                 "enabled": {"type": "boolean"}
             }),
             vec!["routine_id", "enabled"]),
        tool("update_routine",
             "Edit one of your routines. Omitted fields are left alone. \
              Changing the trigger cancels occurrences queued under the old schedule.",
             json!({
                 "routine_id": {"type": "string"},
                 "name": {"type": "string"},
                 "trigger": {"type": "object", "description": "{kind:'cron',expr,tz} | {kind:'interval',seconds} | {kind:'signal',name,from_bot_id?}. A signal trigger waits for emit_signal to announce that name; from_bot_id is a bot id from list_bots, and omitting it reacts to any bot in your project."},
                 "prompt": {"type": "string"},
                 "busy_policy": {"type": "string", "enum": ["skip", "queue_one", "queue_all", "replace"]},
                 "max_duration_seconds": {"type": "integer", "description": "Give up on one run after this long. Defaults to the daemon's scheduler lease."},
                 "max_attempts": {"type": "integer", "description": "Attempts per occurrence, including the first. 1 (the default) never retries."},
             }),
             vec!["routine_id"]),
        tool("emit_signal",
             "Announce that something happened, by name. Routines in your project with a \
              matching signal trigger run in response; nothing happens if none subscribe. \
              Use this instead of expecting other bots to notice you finished a turn.",
             json!({
                 "name": {"type": "string", "description": "Lowercase name, e.g. 'deploy.finished'. Subscribers match it exactly."},
                 "payload": {"type": "object", "description": "Optional context recorded with the signal."}
             }),
             vec!["name"]),
        tool("delete_routine",
             "Delete one of your routines. Its run history is removed and it cannot be restored.",
             json!({ "routine_id": {"type": "string"} }),
             vec!["routine_id"]),
        tool("get_self",
             "Read your own settings: name, avatar, description, instructions, who created you, and the bots you created.",
             json!({}), vec![]),
        tool("update_self",
             "Change your own avatar, description or instructions. Applies immediately; omitted fields are left alone.",
             json!({
                 "avatar": {"type": "string", "description": avatar_hint()},
                 "description": {"type": "string", "description": "Short summary other bots see in list_bots"},
                 "instructions": {"type": "string", "description": "Your standing instructions, appended to your system prompt"}
             }),
             vec![]),
        tool("rename_self",
             "Change the name other bots use to address you. Other bots in the project are told.",
             json!({ "name": {"type": "string"} }),
             vec!["name"]),
        tool("create_bot",
             "Create another bot in your project. Only 'name' is required: call this straight away when asked for a bot, and do not ask what it is for first. Fill in description and instructions from what you already know; leave them out when you do not know, and the new bot is told to ask. It starts immediately, and you may later update or delete bots you created.",
             json!({
                 "name": {"type": "string"},
                 "description": {"type": "string", "description": "Optional. Short summary other bots see in list_bots"},
                 "instructions": {"type": "string", "description": "Optional. Standing instructions for the new bot"},
                 "avatar": {"type": "string", "description": format!("Optional; defaults to a random icon. {}", avatar_hint())}
             }),
             vec!["name"]),
        tool("update_bot",
             "Change the avatar, description or instructions of a bot you created.",
             json!({
                 "name": {"type": "string", "description": "The bot to change"},
                 "description": {"type": "string"},
                 "instructions": {"type": "string"},
                 "avatar": {"type": "string", "description": avatar_hint()}
             }),
             vec!["name"]),
        tool("delete_bot",
             "Delete a bot you created. Its history and workspace are kept, but it cannot be restored. Frees a slot and its name.",
             json!({
                 "name": {"type": "string"},
                 "reason": {"type": "string"}
             }),
             vec!["name"]),
    ]
}

/// The accepted avatar forms, spelled out for the model.
///
/// Built from the shared icon list so a new icon reaches bots without anyone
/// remembering to retype it here.
fn avatar_hint() -> String {
    format!(
        "'icon:<name>' where name is one of {}, or 'color:#rrggbb'",
        bus::avatar::ICONS.join(", ")
    )
}
