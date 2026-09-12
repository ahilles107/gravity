//! Message and conversation requests.

use bus::MessageKind;
use serde_json::{json, Value};

use crate::messaging;

use super::Conn;

impl Conn {
    // ---- messaging ----

    pub(super) fn send_user_message(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let body = Self::str_field(req, "body")?;
        let sender = messaging::user_sender();
        let bot_id = Self::str_field(req, "to_bot_id")?;
        let msg = messaging::send_dm(
            &self.app.db,
            &self.app.events,
            messaging::Dm::new(bot_id, &sender, MessageKind::Chat, body),
        )?;
        self.send(json!({ "type": "message", "req_id": req_id, "message": msg }));
        Ok(())
    }

    pub(super) fn list_messages(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let conversation_id = Self::str_field(req, "conversation_id")?;
        let before = req.get("before_num").and_then(|v| v.as_i64());
        let limit = req.get("limit").and_then(|v| v.as_i64()).unwrap_or(100);
        let messages = self.app.db.list_messages(conversation_id, before, limit)?;
        self.send(json!({ "type": "messages", "req_id": req_id, "messages": messages }));
        Ok(())
    }

    pub(super) fn list_conversations(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let project_id = req.get("project_id").and_then(|v| v.as_str());
        let conversations = self.app.db.list_conversations(project_id)?;
        self.send(
            json!({ "type": "conversations", "req_id": req_id, "conversations": conversations }),
        );
        Ok(())
    }
}
