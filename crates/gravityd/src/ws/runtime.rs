use super::Conn;
use crate::botmgmt::{self, requested_runtime};
use serde_json::{json, Value};

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
        let bot = self
            .app
            .db
            .get_live_bot(bot_id)?
            .ok_or_else(|| anyhow::anyhow!("bot not found"))?;
        // A stand-in's runtime is its machine's to change.
        if crate::peer::remote_bots::is_mirrored(&self.app, &bot) {
            self.update_bot_on_peer(req_id, &json!({ "runtime": runtime.as_str() }), bot);
            return Ok(());
        }
        let bot = match botmgmt::set_bot_runtime(&self.app, &bot, runtime) {
            Ok(bot) => bot,
            Err(error)
                if error
                    .downcast_ref::<botmgmt::RuntimeUnavailable>()
                    .is_some() =>
            {
                self.reply_err(req_id, "runtime_unavailable", &format!("{error:#}"));
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        self.send(json!({ "type": "bot", "req_id": req_id, "bot": self.bot_json(&bot) }));
        Ok(())
    }

    pub(super) fn set_bot_user_chrome(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let Some(enabled) = req.get("enabled").and_then(Value::as_bool) else {
            self.reply_err(req_id, "invalid_request", "'enabled' must be true or false");
            return Ok(());
        };
        let bot = self
            .app
            .db
            .get_live_bot(bot_id)?
            .ok_or_else(|| anyhow::anyhow!("bot not found"))?;
        match botmgmt::set_bot_user_chrome(&self.app, &bot, enabled) {
            Ok(bot) => {
                self.send(json!({ "type": "bot", "req_id": req_id, "bot": self.bot_json(&bot) }));
            }
            Err(error) => self.reply_err(req_id, "invalid_request", &format!("{error:#}")),
        }
        Ok(())
    }

    /// Restarts a bot's session. With `clear`, the new session starts a fresh
    /// conversation; either way it is told what it left unfinished (see
    /// `crate::resume`), and its workspace, memory files and tasks stay. A
    /// linked bot is restarted on its machine.
    fn restart_session(&self, req_id: &Value, req: &Value, clear: bool) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let Some(bot) = self.app.db.get_live_bot(bot_id)? else {
            self.reply_err(req_id, "not_found", "bot not found");
            return Ok(());
        };
        if bot.is_linked() {
            let kind = if clear {
                "clear_bot_session"
            } else {
                "restart_bot"
            };
            self.remote(
                req_id,
                bot,
                json!({ "type": kind }),
                |_| json!({ "type": "ok" }),
            );
            return Ok(());
        }
        let result = if clear {
            let root = std::path::Path::new(&bot.workspace_path).parent();
            self.app.supervisor.clear_session(&bot.id, root)
        } else {
            self.app.supervisor.restart_bot(&bot.id)
        };
        match result {
            Ok(()) => self.send(json!({ "type": "ok", "req_id": req_id })),
            Err(e) => self.reply_err(req_id, "invalid_request", &format!("{e:#}")),
        }
        Ok(())
    }

    pub(super) fn restart_bot(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        self.restart_session(req_id, req, false)
    }

    pub(super) fn clear_bot_session(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        self.restart_session(req_id, req, true)
    }
}
