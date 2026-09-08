//! Implementations of the `gravity-bus` MCP tools.

use std::sync::Arc;

use bus::{
    MessageKind, DEFAULT_TASK_DEADLINE_HOURS, MAX_MESSAGE_BYTES, MAX_TASK_FANOUT, MAX_TASK_HOPS,
    MAX_TASK_REPLIES,
};
use serde_json::{json, Value};

use crate::app::AppState;
use crate::messaging;

use super::tasks::describe_tasks;
use super::{bot_sender, caller};

pub(super) fn send_message(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let to = args
        .get("to")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("'to' is required"))?;
    let body = args
        .get("body")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("'body' is required"))?;
    if body.len() > MAX_MESSAGE_BYTES {
        anyhow::bail!("body exceeds {MAX_MESSAGE_BYTES} bytes");
    }
    let kind = match args.get("kind").and_then(|v| v.as_str()) {
        Some(k) => MessageKind::parse(k)
            .ok_or_else(|| anyhow::anyhow!("unknown kind '{k}' — use 'task', 'reply' or 'note'"))?,
        None => MessageKind::Note,
    };
    match kind {
        MessageKind::Chat => anyhow::bail!(
            "refusing send: kind 'chat' is reserved for user conversations — use \
             'task' (work order), 'reply' (on an open task) or 'note' (FYI)"
        ),
        MessageKind::Done => anyhow::bail!(
            "refusing send: kind 'done' is produced by complete_task — call \
             complete_task with the task_id to report a result"
        ),
        MessageKind::Task | MessageKind::Reply | MessageKind::Note => {}
    }
    let ref_id = args.get("ref").and_then(|v| v.as_str());

    // Hop/origin tracking: extend the chain from the caller's newest open task.
    let open_task = app.db.newest_open_task_for(bot_id)?;
    let (hop, chain) = open_task
        .as_ref()
        .map(|t| (t.hop_count, t.origin_chain.clone()))
        .unwrap_or((0, String::new()));

    // Addressed by bot name, resolved within the caller's project.
    let target = app
        .db
        .get_bot_by_name(&me.project_id, to)?
        .ok_or_else(|| anyhow::anyhow!("no bot named '{to}' in this project"))?;
    if target.id == me.id {
        anyhow::bail!("cannot send a message to yourself");
    }

    match kind {
        // A reply belongs to the open task between the pair — the one the
        // target delegated to me, or failing that the one I delegated to the
        // target. No open task means there is nothing to reply on: results
        // arrive as `done`, and `done` closes the conversation.
        MessageKind::Reply => {
            if let Some(ref_id) = ref_id {
                let referenced = app
                    .db
                    .get_message(ref_id)?
                    .ok_or_else(|| anyhow::anyhow!("no message with id '{ref_id}'"))?;
                if referenced.kind == MessageKind::Done {
                    anyhow::bail!(
                        "refusing send: message {ref_id} is a task result (kind 'done') \
                         and needs no answer — open a new task if it calls for more work"
                    );
                }
            }
            let requested_by_target = open_task
                .as_ref()
                .filter(|t| t.from_bot_id.as_deref() == Some(target.id.as_str()));
            let task = match requested_by_target {
                Some(t) => t.clone(),
                None => app
                    .db
                    .newest_open_task_from(&me.id, &target.id)?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "refusing send: no open task between you and {to} to reply \
                             on — send kind 'task' to ask for work, or 'note' for an \
                             FYI that needs no answer"
                        )
                    })?,
            };
            if !app.db.try_count_task_reply(&task.id, MAX_TASK_REPLIES)? {
                anyhow::bail!(
                    "refusing send: reply limit reached for task {} ({MAX_TASK_REPLIES} \
                     replies) — if the task is assigned to you, finish it with \
                     complete_task; if you delegated it, wait for the result",
                    task.id
                );
            }
            // A reply defaults to referencing the message that opened the
            // task, so the reader sees what it answers.
            let ref_id = ref_id.unwrap_or(task.origin_message_id.as_str());
            let msg = messaging::send_dm(
                &app.db,
                &app.events,
                &target.id,
                &bot_sender(&me),
                kind,
                body,
                Some(ref_id),
            )?;
            Ok(json!({ "message_id": msg.id, "num": msg.num }))
        }
        // A note is an FYI: it opens no task, expects no answer, and budgets
        // nothing.
        MessageKind::Note => {
            let msg = messaging::send_dm(
                &app.db,
                &app.events,
                &target.id,
                &bot_sender(&me),
                kind,
                body,
                ref_id,
            )?;
            Ok(json!({ "message_id": msg.id, "num": msg.num }))
        }
        // A task is a delegation: it extends the chain and opens a task row,
        // so every budget — loop, depth, fan-out — is checked before the
        // message leaves.
        MessageKind::Task => {
            if chain.split(',').any(|hop_id| hop_id == target.id) {
                let requested_by_target = open_task
                    .as_ref()
                    .filter(|t| t.from_bot_id.as_deref() == Some(target.id.as_str()));
                let hint = match requested_by_target {
                    Some(t) => format!(
                        " — {to} delegated task {} to you: report the result with \
                         complete_task, or send kind 'reply' to ask a question",
                        t.id
                    ),
                    None => String::new(),
                };
                anyhow::bail!(
                    "refusing send: {to} is already in this task's origin chain (loop){hint}"
                );
            }
            if hop + 1 > MAX_TASK_HOPS {
                anyhow::bail!(
                    "refusing send: this delegation chain is {hop} hops deep (limit \
                     {MAX_TASK_HOPS}) — do the work yourself, or report what you have \
                     with complete_task"
                );
            }
            let new_chain = if chain.is_empty() {
                me.id.clone()
            } else {
                format!("{chain},{}", me.id)
            };
            let open = app.db.open_tasks_delegated_by(&me.id, Some(&new_chain))?;
            if open.len() as i64 >= MAX_TASK_FANOUT {
                // Naming the tasks is what makes the advice actionable: the
                // ids came back from sends that may be far behind in context.
                anyhow::bail!(
                    "refusing send: you already have {MAX_TASK_FANOUT} open tasks \
                     delegated from this one — wait for a result, ask a delegate \
                     for status with kind 'reply', or close one with cancel_task \
                     before opening another. Open now: {}",
                    describe_tasks(app, &open)?
                );
            }
            let deadline_hours = args
                .get("deadline_hours")
                .and_then(|v| v.as_i64())
                .unwrap_or(DEFAULT_TASK_DEADLINE_HOURS)
                .clamp(1, 168);
            let deadline_at = chrono::Utc::now() + chrono::Duration::hours(deadline_hours);
            let msg = messaging::send_dm(
                &app.db,
                &app.events,
                &target.id,
                &bot_sender(&me),
                kind,
                body,
                ref_id,
            )?;
            let task = app.db.create_task(
                &msg.id,
                Some(&me.id),
                &target.id,
                Some(deadline_at),
                hop + 1,
                &new_chain,
            )?;
            Ok(json!({ "message_id": msg.id, "num": msg.num, "task_id": task.id }))
        }
        MessageKind::Chat | MessageKind::Done => unreachable!("refused above"),
    }
}

pub(super) fn list_bots(app: &Arc<AppState>, bot_id: &str) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let bots: Vec<Value> = app
        .db
        .list_bots(Some(&me.project_id))?
        .into_iter()
        .map(|b| {
            let (state, _) = app.supervisor.state(&b.id);
            json!({
                "id": b.id,
                "name": b.name,
                "avatar": b.avatar,
                "description": b.description,
                "status": state.as_str(),
                "created_by_me": b.created_by_bot_id.as_deref() == Some(bot_id)
            })
        })
        .collect();
    Ok(json!({ "bots": bots }))
}

pub(super) fn check_inbox(app: &Arc<AppState>, bot_id: &str) -> anyhow::Result<Value> {
    let msgs = app.db.consume_inbox(bot_id)?;
    // The task ids `complete_task` needs; without them a delegated task is a
    // message the assignee can read but never answer. One query for the whole
    // batch; newest-first order means the first id seen per message wins.
    let mut task_by_msg = std::collections::HashMap::new();
    for task in app.db.open_tasks_for(bot_id)? {
        task_by_msg
            .entry(task.origin_message_id.clone())
            .or_insert(task.id);
    }
    let mut rendered: Vec<Value> = Vec::with_capacity(msgs.len());
    for m in &msgs {
        let mut item = json!({
            "message_id": m.id,
            "num": m.num,
            "from": m.sender.name,
            "kind": m.kind.as_str(),
            "body": m.body,
            "created_at": m.created_at.to_rfc3339()
        });
        if let Some(task_id) = task_by_msg.get(m.id.as_str()) {
            item["task_id"] = json!(task_id);
        }
        rendered.push(item);
    }
    // The other half of the ledger: tasks this bot is waiting on. Without it
    // the only source of a delegated task's id is the send that opened it,
    // which is gone once the session's context has moved on — and with it any
    // way to chase or cancel the task.
    let mut delegated: Vec<Value> = Vec::new();
    for task in app.db.open_tasks_delegated_by(bot_id, None)? {
        let to = app
            .db
            .get_bot(&task.to_bot_id)?
            .map(|b| b.name)
            .unwrap_or_else(|| task.to_bot_id.clone());
        delegated.push(json!({
            "task_id": task.id,
            "to": to,
            "opened_at": task.created_at.to_rfc3339()
        }));
    }
    let mut out = json!({ "messages": rendered });
    if !delegated.is_empty() {
        out["delegated_tasks"] = json!(delegated);
    }
    Ok(out)
}
