//! A bot's tasks for its Tasks panel: what it is working on, what it is
//! waiting on, and what finished.

use serde_json::{json, Value};

use super::Conn;

const DEFAULT_LIMIT: i64 = 100;
const MAX_REQUEST_CHARS: usize = 400;
const MAX_RESULT_CHARS: usize = 600;

impl Conn {
    pub(super) fn list_tasks(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let limit = req
            .get("limit")
            .and_then(Value::as_i64)
            .unwrap_or(DEFAULT_LIMIT)
            .clamp(1, 500);
        let db = &self.app.db;
        let mut tasks = Vec::new();
        for task in db.tasks_involving(bot_id, limit)? {
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
                Some(peer_id) => db.get_peer(peer_id)?.map(|p| p.name),
                None => None,
            };
            let request = db
                .get_message(&task.origin_message_id)?
                .map(|m| crate::chat::truncate(&m.body, MAX_REQUEST_CHARS))
                .unwrap_or_default();
            let result = db.task_result(&task.origin_message_id)?;
            tasks.push(json!({
                "id": task.id,
                "state": task.state,
                "role": if assigned { "assigned" } else { "delegated" },
                "other": {
                    "id": other_id,
                    "name": other.as_ref().map_or_else(|| "you".to_string(), crate::db::Db::display_name),
                    "machine": machine,
                },
                "request": request,
                "result": result.as_ref().map(|m| crate::chat::truncate(&m.body, MAX_RESULT_CHARS)),
                "created_at": task.created_at.to_rfc3339(),
                "deadline_at": task.deadline_at.map(|t| t.to_rfc3339()),
                "closed_at": result.map(|m| m.created_at.to_rfc3339()),
            }));
        }
        self.send(json!({ "type": "tasks", "req_id": req_id, "bot_id": bot_id, "tasks": tasks }));
        Ok(())
    }
}
