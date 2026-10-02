//! A project's shared git repository, which workers check out and push to.

use serde_json::{json, Value};

use super::Conn;
use crate::botmgmt;

impl Conn {
    /// Set the repository with `url` (and `branch`, default `main`), or clear
    /// it with `url: null`. Bots' system prompts are rewritten to match, for
    /// their next start.
    pub(super) fn set_project_repo(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let project_id = Self::str_field(req, "project_id")?;
        let Some(project) = self.app.db.get_live_project(project_id)? else {
            self.reply_err(req_id, "not_found", "project not found or already deleted");
            return Ok(());
        };
        let repo = match req.get("url").and_then(Value::as_str) {
            Some(url) => {
                let branch = req.get("branch").and_then(Value::as_str);
                match bus::ProjectRepo::parse(url, branch) {
                    Ok(repo) => Some(repo),
                    Err(message) => {
                        self.reply_err(req_id, "invalid_request", &message);
                        return Ok(());
                    }
                }
            }
            None => None,
        };
        self.app.db.set_project_repo(&project.id, repo.as_ref())?;
        for bot in self
            .app
            .db
            .list_bots(Some(&project.id))?
            .into_iter()
            .filter(|b| !b.is_linked())
        {
            if let Err(error) = botmgmt::reprovision(&self.app, &bot) {
                tracing::warn!(bot_id = %bot.id, %error, "system.md not rewritten for the repository");
            }
        }
        self.app.events.push(crate::events::Push::ProjectUpdated {
            project: project.clone(),
        });
        self.send(json!({
            "type": "project", "req_id": req_id,
            "project": super::project_view(&self.app, &project)
        }));
        Ok(())
    }
}
