//! Chat requests: a bot's turns, a step's detail, images and files. They read
//! transcripts and files, so each runs off the connection's task.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::app::AppState;
use crate::chat::files::{self, Scope};

use super::Conn;

/// Turns sent when the client does not ask for a number.
const DEFAULT_PAGE: usize = 30;
const MAX_PAGE: usize = 200;

impl Conn {
    pub(super) fn list_chat(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?.to_string();
        let before = req
            .get("before")
            .and_then(Value::as_str)
            .map(str::to_string);
        let limit = req
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_PAGE, |n| (n as usize).clamp(1, MAX_PAGE));
        if let Some(bot) = self.linked(&bot_id) {
            let frame = json!({ "type": "chat", "before": before, "limit": limit });
            self.remote(req_id, bot, frame, move |result| {
                json!({
                    "type": "chat", "bot_id": bot_id,
                    "turns": crate::peer::chat::localize(result["turns"].clone(), &bot_id),
                    "has_more": result["has_more"]
                })
            });
            return Ok(());
        }
        self.blocking(req_id, move |app| {
            let bot = live_bot(app, &bot_id)?;
            let (turns, has_more) = app.chat.page(app, &bot, before.as_deref(), limit)?;
            Ok(json!({ "type": "chat", "bot_id": bot_id, "turns": turns, "has_more": has_more }))
        });
        Ok(())
    }

    pub(super) fn get_chat_step(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?.to_string();
        let item_id = Self::str_field(req, "item_id")?.to_string();
        if let Some(bot) = self.linked(&bot_id) {
            let frame = json!({ "type": "chat_step", "item_id": item_id });
            self.remote(req_id, bot, frame, move |result| {
                json!({ "type": "chat_step", "bot_id": bot_id, "item_id": item_id, "detail": result["detail"] })
            });
            return Ok(());
        }
        self.blocking(req_id, move |app| {
            let bot = live_bot(app, &bot_id)?;
            let detail = app.chat.step(app, &bot, &item_id)?;
            Ok(json!({
                "type": "chat_step", "bot_id": bot_id, "item_id": item_id, "detail": detail
            }))
        });
        Ok(())
    }

    pub(super) fn get_chat_image(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?.to_string();
        let image_id = Self::str_field(req, "image_id")?.to_string();
        if let Some(bot) = self.linked(&bot_id) {
            let frame = json!({ "type": "chat_image", "image_id": image_id });
            self.remote(req_id, bot, frame, move |result| {
                json!({ "type": "file", "file": {
                    "name": image_id, "mime": result["mime"], "base64": result["base64"], "truncated": false
                } })
            });
            return Ok(());
        }
        self.blocking(req_id, move |app| {
            let bot = live_bot(app, &bot_id)?;
            let (mime, base64) = app.chat.image(app, &bot, &image_id)?;
            Ok(json!({
                "type": "file",
                "file": { "name": image_id, "mime": mime, "base64": base64, "truncated": false }
            }))
        });
        Ok(())
    }

    pub(super) fn list_artifacts(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let project_id = Self::str_field(req, "project_id")?.to_string();
        self.blocking(req_id, move |app| {
            let project = live_project(app, &project_id)?;
            let artifacts = files::list_artifacts(app, &project);
            Ok(json!({ "type": "artifacts", "project_id": project_id, "artifacts": artifacts }))
        });
        Ok(())
    }

    /// A file from the project's artifacts (`project_id`), or from a bot's
    /// directory and its project's artifacts (`bot_id`).
    pub(super) fn read_file(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let path = Self::str_field(req, "path")?.to_string();
        let bot_id = req
            .get("bot_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(bot) = bot_id.as_deref().and_then(|id| self.linked(id)) {
            let frame = json!({ "type": "read_file", "path": path });
            self.remote(
                req_id,
                bot,
                frame,
                |result| json!({ "type": "file", "file": result["file"] }),
            );
            return Ok(());
        }
        let project_id = req
            .get("project_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        self.blocking(req_id, move |app| {
            let file = match (bot_id, project_id) {
                (Some(bot_id), _) => {
                    let bot = live_bot(app, &bot_id)?;
                    let project = live_project(app, &bot.project_id)?;
                    files::read_file(app, Scope::Bot(&bot, &project), &path)?
                }
                (None, Some(project_id)) => {
                    let project = live_project(app, &project_id)?;
                    files::read_file(app, Scope::Project(&project), &path)?
                }
                (None, None) => anyhow::bail!("'bot_id' or 'project_id' is required"),
            };
            Ok(json!({ "type": "file", "file": file }))
        });
        Ok(())
    }

    /// One chunk of a file the owner attached in the composer, saved into the
    /// project's artifacts. `control`: it writes to the bots' shared folder.
    pub(super) fn write_artifact(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        use base64::Engine;
        let project_id = Self::str_field(req, "project_id")?.to_string();
        let name = Self::str_field(req, "name")?.to_string();
        let upload_id = req
            .get("upload_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let last = req.get("last").and_then(Value::as_bool).unwrap_or(true);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(Self::str_field(req, "base64")?)
            .map_err(|_| anyhow::anyhow!("'base64' is not valid base64"))?;
        self.blocking(req_id, move |app| {
            let project = live_project(app, &project_id)?;
            let upload =
                files::append_upload(app, &project, upload_id.as_deref(), &name, &bytes, last)?;
            Ok(json!({ "type": "upload", "upload": upload }))
        });
        Ok(())
    }

    /// The bot, when it is linked: its chat lives on its peer.
    fn linked(&self, bot_id: &str) -> Option<bus::Bot> {
        self.app
            .db
            .get_live_bot(bot_id)
            .ok()
            .flatten()
            .filter(bus::Bot::is_linked)
    }

    /// Asks a linked bot's peer and replies with `reply(result)`, or with an
    /// `unavailable` error when the peer is offline or refuses.
    fn remote(
        &self,
        req_id: &Value,
        bot: bus::Bot,
        frame: Value,
        reply: impl FnOnce(Value) -> Value + Send + 'static,
    ) {
        let (app, out, req_id) = (self.app.clone(), self.out.clone(), req_id.clone());
        tokio::spawn(async move {
            let response = match crate::peer::chat::ask(&app, &bot, frame).await {
                Ok(result) => {
                    let mut response = reply(result);
                    response["req_id"] = req_id;
                    response
                }
                Err(e) => json!({
                    "type": "error", "req_id": req_id, "code": "unavailable",
                    "message": format!("{e:#}")
                }),
            };
            let _ = out.send(response);
        });
    }

    /// Runs `work` on a blocking thread and replies with its result, or with
    /// a `not_found` error the client can show.
    pub(super) fn blocking(
        &self,
        req_id: &Value,
        work: impl FnOnce(&Arc<AppState>) -> anyhow::Result<Value> + Send + 'static,
    ) {
        let (app, out, req_id) = (self.app.clone(), self.out.clone(), req_id.clone());
        tokio::task::spawn_blocking(move || {
            let reply = match work(&app) {
                Ok(mut reply) => {
                    reply["req_id"] = req_id;
                    reply
                }
                Err(e) => json!({
                    "type": "error", "req_id": req_id, "code": "not_found",
                    "message": format!("{e:#}")
                }),
            };
            let _ = out.send(reply);
        });
    }
}

fn live_bot(app: &AppState, bot_id: &str) -> anyhow::Result<bus::Bot> {
    app.db
        .get_live_bot(bot_id)?
        .ok_or_else(|| anyhow::anyhow!("bot not found"))
}

fn live_project(app: &AppState, project_id: &str) -> anyhow::Result<bus::Project> {
    app.db
        .get_project(project_id)?
        .filter(|p| p.deleted_at.is_none())
        .ok_or_else(|| anyhow::anyhow!("project not found"))
}
