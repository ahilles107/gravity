//! Project and bot requests.

use serde_json::{json, Value};

use std::path::Path;

use crate::activity;
use crate::botmgmt::{self, IdentityEdit};
use crate::db::Actor;
use crate::projectmgmt;

use super::Conn;

/// How many times an unnamed creation re-picks its placeholder after losing a
/// race for it. Two clients colliding is already unlikely; three colliding on
/// the retry too is not worth planning for beyond a clear error.
const NAME_ATTEMPTS: usize = 3;

impl Conn {
    // ---- projects / bots ----

    /// Allocate the friendly placeholder used by one-click bot creation.
    ///
    /// Only live bots hold a name and a project holds at most
    /// `max_bots_per_project` of them, so one of the `max + 1` candidates is
    /// always free — the loop cannot fall through while both of those hold.
    fn default_bot_name(&self, project_id: &str) -> anyhow::Result<String> {
        for number in 1..=self.app.cfg.max_bots_per_project.saturating_add(1) {
            let candidate = if number == 1 {
                "New Bot".to_string()
            } else {
                format!("New Bot {number}")
            };
            if self
                .app
                .db
                .get_bot_by_name(project_id, &candidate)?
                .is_none()
            {
                return Ok(candidate);
            }
        }
        anyhow::bail!("cannot allocate a default bot name in this project")
    }

    pub(super) fn list_projects(&self, req_id: &Value) -> anyhow::Result<()> {
        let projects: Vec<Value> = self
            .app
            .db
            .list_projects()?
            .iter()
            .map(super::project_view)
            .collect();
        self.send(json!({ "type": "projects", "req_id": req_id, "projects": projects }));
        Ok(())
    }

    pub(super) fn create_project(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let name = Self::str_field(req, "name")?;
        match projectmgmt::create_project(&self.app, name) {
            Ok(project) => {
                self.send(json!({
                    "type": "project", "req_id": req_id,
                    "project": super::project_view(&project)
                }));
            }
            Err(e) => self.reply_err(req_id, "invalid_request", &e.to_string()),
        }
        Ok(())
    }

    pub(super) fn update_project(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let project_id = Self::str_field(req, "project_id")?;
        let Some(project) = self.app.db.get_live_project(project_id)? else {
            self.reply_err(req_id, "not_found", "project not found or already deleted");
            return Ok(());
        };
        let name = Self::str_field(req, "name")?;
        match projectmgmt::rename_project(&self.app, &project, name) {
            Ok(renamed) => {
                self.send(json!({
                    "type": "project", "req_id": req_id,
                    "project": super::project_view(&renamed)
                }));
            }
            Err(e) => self.reply_err(req_id, "invalid_request", &e.to_string()),
        }
        Ok(())
    }

    /// Archive a project and every bot in it. Archival, not destructive: see
    /// `projectmgmt::archive_project`.
    pub(super) fn delete_project(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let project_id = Self::str_field(req, "project_id")?;
        let Some(project) = self.app.db.get_live_project(project_id)? else {
            self.reply_err(req_id, "not_found", "project not found or already deleted");
            return Ok(());
        };
        projectmgmt::archive_project(&self.app, &project, &Actor::User)?;
        self.send(json!({ "type": "ok", "req_id": req_id }));
        Ok(())
    }

    pub(super) fn list_bots(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let project_id = req.get("project_id").and_then(|v| v.as_str());
        let bots: Vec<Value> = self
            .app
            .db
            .list_bots(project_id)?
            .iter()
            .map(|b| self.bot_json(b))
            .collect();
        self.send(json!({ "type": "bots", "req_id": req_id, "bots": bots }));
        Ok(())
    }

    /// One preview line per bot: the newer of its last Claude Code turn and its
    /// last bus message. Bots that have said nothing yet are simply omitted.
    ///
    /// Answered off the connection's own task. This reads the tail of every
    /// bot's transcript from disk, tens of milliseconds on an idle machine and
    /// far more on a busy one, and a connection handles its frames in order:
    /// left inline it would hold every keystroke typed behind it.
    pub(super) fn list_bot_activity(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let project_id = req
            .get("project_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let app = self.app.clone();
        let out = self.out.clone();
        let req_id = req_id.clone();
        tokio::task::spawn_blocking(move || {
            let home = &app.cfg.user_home;
            let reply = match app.db.list_bots(project_id.as_deref()) {
                Ok(bots) => {
                    let items: Vec<activity::BotActivity> = bots
                        .iter()
                        .filter_map(|bot| {
                            let workspace = Path::new(&bot.workspace_path);
                            let found = activity::for_bot(&app.db, home, &bot.id, workspace)?;
                            Some(found.for_wire(&bot.id))
                        })
                        .collect();
                    json!({ "type": "bot_activity", "req_id": req_id, "activity": items })
                }
                Err(e) => json!({
                    "type": "error", "req_id": req_id, "code": "internal",
                    "message": e.to_string()
                }),
            };
            let _ = out.send(reply);
        });
        Ok(())
    }

    pub(super) fn create_bot(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let project_id = Self::str_field(req, "project_id")?;
        // An explicit `null` is how a client with no name to give says so, so
        // it takes the placeholder path rather than failing as a bad string.
        let supplied = match req.get("name") {
            None | Some(Value::Null) => None,
            Some(Value::String(name)) => Some(name.clone()),
            Some(_) => {
                self.reply_err(req_id, "invalid_request", "'name' must be a string");
                return Ok(());
            }
        };
        // Allocating a placeholder and inserting it are separate steps, so
        // another connection can take the name in between. The user never saw
        // that name, so a lost race is retried on a fresh one instead of being
        // reported as a clash.
        let attempts = if supplied.is_some() { 1 } else { NAME_ATTEMPTS };
        for attempt in 1..=attempts {
            let name = match &supplied {
                Some(name) => name.clone(),
                None => match self.default_bot_name(project_id) {
                    Ok(name) => name,
                    Err(e) => {
                        self.reply_err(req_id, "invalid_request", &e.to_string());
                        return Ok(());
                    }
                },
            };
            let edit = IdentityEdit {
                name: Some(&name),
                description: req.get("description").and_then(|v| v.as_str()),
                instructions: req.get("instructions").and_then(|v| v.as_str()),
                avatar: req.get("avatar").and_then(|v| v.as_str()),
            };
            match botmgmt::create_bot(&self.app, project_id, &edit, None, &Actor::User) {
                Ok(created) => {
                    self.send(json!({
                        "type": "bot", "req_id": req_id,
                        "bot": self.bot_json(&created.bot)
                    }));
                    return Ok(());
                }
                Err(e) => {
                    let lost_race =
                        attempt < attempts && e.downcast_ref::<botmgmt::NameTaken>().is_some();
                    if !lost_race {
                        self.reply_err(req_id, "invalid_request", &e.to_string());
                        return Ok(());
                    }
                }
            }
        }
        self.reply_err(
            req_id,
            "invalid_request",
            "could not claim a default bot name; try again",
        );
        Ok(())
    }

    pub(super) fn update_bot(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let bot = self
            .app
            .db
            .get_live_bot(bot_id)?
            .ok_or_else(|| anyhow::anyhow!("bot not found"))?;
        let edit = IdentityEdit {
            name: req.get("name").and_then(|v| v.as_str()),
            description: req.get("description").and_then(|v| v.as_str()),
            instructions: req.get("instructions").and_then(|v| v.as_str()),
            avatar: req.get("avatar").and_then(|v| v.as_str()),
        };
        match botmgmt::apply_identity_edit(&self.app, &bot, &edit, &Actor::User) {
            Ok(updated) => {
                self.send(
                    json!({ "type": "bot", "req_id": req_id, "bot": self.bot_json(&updated) }),
                );
            }
            Err(e) => self.reply_err(req_id, "invalid_request", &e.to_string()),
        }
        Ok(())
    }

    /// Archive a bot. Users may delete any bot; bots may only delete their own
    /// children, which the MCP path enforces before calling the same code.
    pub(super) fn delete_bot(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let reason = req.get("reason").and_then(|v| v.as_str());
        let Some(bot) = self.app.db.get_live_bot(bot_id)? else {
            self.reply_err(req_id, "not_found", "bot not found or already deleted");
            return Ok(());
        };
        botmgmt::archive_bot(&self.app, &bot, &Actor::User, reason)?;
        self.send(json!({ "type": "ok", "req_id": req_id }));
        Ok(())
    }

    // ---- identity history ----

    pub(super) fn list_bot_revisions(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let limit = req.get("limit").and_then(|v| v.as_i64()).unwrap_or(50);
        let revisions = self.app.db.list_bot_revisions(bot_id, limit)?;
        self.send(json!({
            "type": "bot_revisions", "req_id": req_id, "bot_revisions": revisions
        }));
        Ok(())
    }

    /// Undo one identity change. This is the counterweight to bots editing
    /// themselves unsupervised: nothing asks first, but everything is
    /// reversible.
    pub(super) fn revert_bot_revision(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let revision_id = Self::str_field(req, "revision_id")?;
        match botmgmt::revert_revision(&self.app, revision_id) {
            Ok(bot) => {
                self.send(json!({ "type": "bot", "req_id": req_id, "bot": self.bot_json(&bot) }));
            }
            Err(e) => self.reply_err(req_id, "invalid_request", &e.to_string()),
        }
        Ok(())
    }
}
