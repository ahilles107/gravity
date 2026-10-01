use super::Conn;
use crate::events::Push;
use bus::BotRuntime;
use serde_json::{json, Value};

pub(super) fn requested_runtime(req: &Value) -> anyhow::Result<Option<BotRuntime>> {
    req.get("runtime")
        .map(|value| serde_json::from_value(value.clone()).map_err(anyhow::Error::from))
        .transpose()
}

impl Conn {
    pub(super) fn set_bot_runtime(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let runtime = match requested_runtime(req) {
            Ok(Some(runtime)) => runtime,
            result => {
                let message = result
                    .err()
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "runtime is required".to_string());
                self.reply_err(req_id, "invalid_request", &message);
                return Ok(());
            }
        };
        let mut bot = self
            .app
            .db
            .get_live_bot(bot_id)?
            .ok_or_else(|| anyhow::anyhow!("bot not found"))?;
        if bot.runtime != runtime {
            self.app.db.set_bot_runtime(bot_id, runtime)?;
            if let Err(error) = self.app.supervisor.restart_bot(bot_id) {
                self.app.db.set_bot_runtime(bot_id, bot.runtime)?;
                return Err(error);
            }
            bot.runtime = runtime;
            self.app.events.push(Push::BotUpdated { bot: bot.clone() });
        }
        self.send(json!({ "type": "bot", "req_id": req_id, "bot": self.bot_json(&bot) }));
        Ok(())
    }
}
