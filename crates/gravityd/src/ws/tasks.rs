//! A bot's tasks for its Tasks panel: what it is working on, what it is
//! waiting on, and what finished.

use bus::Task;
use serde_json::{json, Value};

use crate::db::Db;

use super::Conn;

const DEFAULT_LIMIT: i64 = 100;
/// Previews in the list; the full text is one `get_task` away.
const MAX_REQUEST_CHARS: usize = 400;
const MAX_RESULT_CHARS: usize = 600;

/// How much of a task's text to send.
#[derive(Clone, Copy)]
enum Length {
    Preview,
    Full,
}

fn clip(text: &str, max: usize, length: Length) -> (String, bool) {
    match length {
        Length::Full => (text.to_string(), false),
        Length::Preview => (crate::chat::truncate(text, max), text.chars().count() > max),
    }
}

/// One task as the panel shows it, from `bot_id`'s side.
fn task_json(db: &Db, task: &Task, bot_id: &str, length: Length) -> anyhow::Result<Value> {
    let assigned = task.to_bot_id == bot_id;
    let other_id = if assigned {
        task.from_bot_id.clone()
    } else {
        Some(task.to_bot_id.clone())
    };
    let other = match &other_id {
        Some(id) => db.get_bot(id)?,
        None => None,
    };
    let machine = match other.as_ref().and_then(|b| b.peer_id.as_deref()) {
        Some(peer_id) => db.get_peer(peer_id)?.map(|p| Db::display_peer_name(&p)),
        None => None,
    };
    let request_body = db
        .get_message(&task.origin_message_id)?
        .map(|m| m.body)
        .unwrap_or_default();
    let (request, request_truncated) = clip(&request_body, MAX_REQUEST_CHARS, length);
    let result = db.task_result(&task.origin_message_id)?;
    let clipped = result
        .as_ref()
        .map(|m| clip(&m.body, MAX_RESULT_CHARS, length));
    Ok(json!({
        "id": task.id,
        "state": task.state,
        "role": if assigned { "assigned" } else { "delegated" },
        "other": {
            "id": other_id,
            "name": other.as_ref().map_or_else(|| "you".to_string(), Db::display_name),
            "machine": machine,
        },
        "request": request,
        "request_truncated": request_truncated,
        "result": clipped.as_ref().map(|(text, _)| text),
        "result_truncated": clipped.is_some_and(|(_, cut)| cut),
        "created_at": task.created_at.to_rfc3339(),
        "deadline_at": task.deadline_at.map(|t| t.to_rfc3339()),
        "closed_at": result.map(|m| m.created_at.to_rfc3339()),
    }))
}

impl Conn {
    pub(super) fn list_tasks(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let limit = req
            .get("limit")
            .and_then(Value::as_i64)
            .unwrap_or(DEFAULT_LIMIT)
            .clamp(1, 500);
        let db = &self.app.db;
        let tasks = db
            .tasks_involving(bot_id, limit)?
            .iter()
            .map(|task| task_json(db, task, bot_id, Length::Preview))
            .collect::<anyhow::Result<Vec<_>>>()?;
        self.send(json!({ "type": "tasks", "req_id": req_id, "bot_id": bot_id, "tasks": tasks }));
        Ok(())
    }

    /// One task with its whole request and result, from `bot_id`'s side.
    pub(super) fn get_task(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let task_id = Self::str_field(req, "task_id")?;
        let db = &self.app.db;
        let Some(task) = db
            .get_task(task_id)?
            .filter(|t| t.to_bot_id == bot_id || t.from_bot_id.as_deref() == Some(bot_id))
        else {
            self.reply_err(req_id, "not_found", "no such task for this bot");
            return Ok(());
        };
        let task = task_json(db, &task, bot_id, Length::Full)?;
        self.send(json!({ "type": "task", "req_id": req_id, "task": task }));
        Ok(())
    }
}
