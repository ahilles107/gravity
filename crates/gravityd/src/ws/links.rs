//! Linked projects on the control plane: listing a peer's projects, linking
//! and unlinking, and bots created or managed on the peer. Each request asks
//! the peer, so it is answered off the connection's own task. See "Linked
//! projects" in `docs/peer-bots.md`.

use std::future::Future;

use serde_json::{json, Value};

use crate::peer::{error_code, links, remote_bots};

use super::Conn;

impl Conn {
    /// Answers `req_id` with whatever `work` produces, or its error with the
    /// error's own code.
    fn answer_later(
        &self,
        req_id: &Value,
        work: impl Future<Output = anyhow::Result<Value>> + Send + 'static,
    ) {
        let (out, req_id) = (self.out.clone(), req_id.clone());
        tokio::spawn(async move {
            let mut reply = match work.await {
                Ok(reply) => reply,
                Err(e) => json!({
                    "type": "error", "code": error_code(&e, "invalid_request"),
                    "message": format!("{e:#}")
                }),
            };
            reply["req_id"] = req_id;
            let _ = out.send(reply);
        });
    }

    pub(super) fn list_peer_projects(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let peer_id = Self::str_field(req, "peer_id")?.to_string();
        let app = self.app.clone();
        self.answer_later(req_id, async move {
            let result = app
                .peers
                .request(&peer_id, json!({ "type": "list_projects" }))
                .await?;
            Ok(json!({
                "type": "peer_projects", "peer_id": peer_id,
                "projects": result.get("projects").cloned().unwrap_or_else(|| json!([]))
            }))
        });
        Ok(())
    }

    pub(super) fn link_project(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let project_id = Self::str_field(req, "project_id")?.to_string();
        let peer_id = Self::str_field(req, "peer_id")?.to_string();
        let remote_id = optional(req, "remote_project_id");
        let remote_name = optional(req, "remote_name");
        let app = self.app.clone();
        self.answer_later(req_id, async move {
            let project = links::link(
                &app,
                &project_id,
                &peer_id,
                remote_id.as_deref(),
                remote_name.as_deref(),
            )
            .await?;
            Ok(json!({ "type": "project", "project": super::project_view(&app, &project) }))
        });
        Ok(())
    }

    pub(super) fn unlink_project(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let project_id = Self::str_field(req, "project_id")?.to_string();
        let peer_id = Self::str_field(req, "peer_id")?.to_string();
        let app = self.app.clone();
        self.answer_later(req_id, async move {
            let project = links::unlink(&app, &project_id, &peer_id).await?;
            Ok(json!({ "type": "project", "project": super::project_view(&app, &project) }))
        });
        Ok(())
    }

    /// `create_bot` with `peer_id`: the bot is created on the peer, in the
    /// project linked with this one, and the reply is its stand-in here.
    pub(super) fn create_bot_on_peer(
        &self,
        req_id: &Value,
        req: &Value,
        peer_id: &str,
    ) -> anyhow::Result<()> {
        let project_id = Self::str_field(req, "project_id")?.to_string();
        let mut identity = remote_bots::fields(req, true);
        // The stand-ins here mirror every bot there, so a name free here is
        // free there too.
        if identity.get("name").is_none() {
            identity["name"] = json!(self.default_bot_name(&project_id)?);
        }
        let (app, peer_id) = (self.app.clone(), peer_id.to_string());
        self.answer_later(req_id, async move {
            let bot = remote_bots::create(&app, &project_id, &peer_id, identity, None).await?;
            Ok(json!({ "type": "bot", "bot": super::bot_view(&app, &bot) }))
        });
        Ok(())
    }

    /// `update_bot` on a stand-in in a linked project edits the bot on its
    /// machine; the stand-in follows.
    pub(super) fn update_bot_on_peer(&self, req_id: &Value, req: &Value, stand_in: bus::Bot) {
        let identity = remote_bots::fields(req, true);
        let app = self.app.clone();
        self.answer_later(req_id, async move {
            let bot = remote_bots::update(&app, &stand_in, identity, None).await?;
            Ok(json!({ "type": "bot", "bot": super::bot_view(&app, &bot) }))
        });
    }

    /// `delete_bot` on a stand-in in a linked project deletes the bot on its
    /// machine; a stand-in alone would come back with the next roster.
    pub(super) fn delete_bot_on_peer(&self, req_id: &Value, req: &Value, stand_in: bus::Bot) {
        let reason = optional(req, "reason");
        let app = self.app.clone();
        self.answer_later(req_id, async move {
            remote_bots::delete(&app, &stand_in, reason.as_deref(), None).await?;
            Ok(json!({ "type": "ok" }))
        });
    }
}

fn optional(req: &Value, key: &str) -> Option<String> {
    req.get(key).and_then(Value::as_str).map(str::to_string)
}
