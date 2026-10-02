//! A bot's own browser on the control plane: watching it live, and what the
//! bot did with it. See "Browser" in `docs/protocol.md`.

use serde_json::{json, Value};

use super::Conn;

const DEFAULT_ACTIVITY: usize = 100;
const MAX_ACTIVITY: usize = 500;

impl Conn {
    /// Starts streaming a bot's browser to this connection: `browser_tabs`
    /// when its tabs change, `browser_frame` with each new screen. `tab_id`
    /// picks a tab to show; without it the view follows the bot. A connection
    /// watches one bot at a time.
    pub(super) fn watch_browser(&mut self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let Some(bot) = self.app.db.get_live_bot(bot_id)? else {
            self.reply_err(req_id, "not_found", "bot not found");
            return Ok(());
        };
        let tab = req
            .get("tab_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(old) = self.browser_watch.take() {
            old.abort();
        }
        self.send(json!({ "type": "ok", "req_id": req_id }));
        self.browser_watch = Some(tokio::spawn(crate::browser::view::watch(
            self.app.clone(),
            bot,
            tab,
            self.out.clone(),
        )));
        Ok(())
    }

    pub(super) fn unwatch_browser(&mut self, req_id: &Value) -> anyhow::Result<()> {
        if let Some(old) = self.browser_watch.take() {
            old.abort();
        }
        self.send(json!({ "type": "ok", "req_id": req_id }));
        Ok(())
    }

    /// The bot's browser actions, newest first, each with the turn it
    /// belongs to and what started that turn.
    pub(super) fn list_browser_activity(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?.to_string();
        let limit = req
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_ACTIVITY, |n| (n as usize).clamp(1, MAX_ACTIVITY));
        self.blocking(req_id, move |app| {
            let bot = app
                .db
                .get_live_bot(&bot_id)?
                .ok_or_else(|| anyhow::anyhow!("bot not found"))?;
            let activity = app.chat.browser_activity(app, &bot, limit)?;
            Ok(json!({ "type": "browser_activity", "bot_id": bot_id, "activity": activity }))
        });
        Ok(())
    }
}
