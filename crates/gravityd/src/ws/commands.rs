//! The commands a bot is running and has run, for its Commands panel. See
//! `crate::chat::commands`.

use serde_json::{json, Value};

use super::Conn;

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 500;

impl Conn {
    pub(super) fn list_bot_commands(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?.to_string();
        let limit = req
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_LIMIT, |n| (n as usize).clamp(1, MAX_LIMIT));
        // A linked bot runs its commands on its machine, which reads them.
        if let Some(bot) = self.linked(&bot_id) {
            let frame = json!({ "type": "bot_commands", "limit": limit });
            self.remote(req_id, bot, frame, move |result| {
                json!({ "type": "bot_commands", "bot_id": bot_id, "commands": result["commands"] })
            });
            return Ok(());
        }
        self.blocking(req_id, move |app| {
            let bot = app
                .db
                .get_live_bot(&bot_id)?
                .ok_or_else(|| anyhow::anyhow!("bot not found"))?;
            let commands = app.chat.commands(app, &bot, limit)?;
            Ok(json!({ "type": "bot_commands", "bot_id": bot_id, "commands": commands }))
        });
        Ok(())
    }
}
