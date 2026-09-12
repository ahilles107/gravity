//! Publishing rulings, tags, and naming a project's lead bot.

use serde_json::{json, Value};

use crate::decisions;

use super::Conn;

impl Conn {
    /// Publish one or several answered decisions, each with its own notify set.
    ///
    /// Batched because the owner answers in batches: the live projects show
    /// several rulings arriving in one typed message, and publishing each
    /// keystroke would send a bot half a decision.
    pub(super) fn publish_decisions(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let items = req
            .get("items")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("'items' is required"))?;
        let mut results = Vec::with_capacity(items.len());
        for item in items {
            let decision_id = Self::str_field(item, "decision_id")?;
            // An inline ruling is the one-step path: answer and publish in the
            // same action, for an owner who did not draft first.
            if let Some(text) = item.get("ruling_text").and_then(|v| v.as_str()) {
                decisions::answer(
                    &self.app,
                    &self.owner(),
                    decision_id,
                    item.get("ruling_option").and_then(|v| v.as_str()),
                    text,
                    item.get("ruling_reason").and_then(|v| v.as_str()),
                )?;
            }
            let notify: Option<Vec<String>> = item
                .get("notify_bot_ids")
                .and_then(|v| v.as_array())
                .map(|ids| {
                    ids.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                });
            let outcome = decisions::publish(&self.app, decision_id, notify.as_deref())?;
            results.push(json!({
                "decision_id": decision_id,
                "notified": outcome.notified,
                // Named with the reason, so the owner is not left believing a
                // bot was told when it could not be.
                "skipped": outcome
                    .skipped
                    .iter()
                    .map(|(name, reason)| json!({ "bot": name, "reason": reason }))
                    .collect::<Vec<_>>()
            }));
        }
        self.send(json!({
            "type": "publish_result", "req_id": req_id, "results": results
        }));
        Ok(())
    }

    pub(super) fn set_decision_tags(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let tags: Vec<String> = req
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let view = decisions::set_tags(
            &self.app,
            &self.owner(),
            Self::str_field(req, "decision_id")?,
            &tags,
        )?;
        self.send(json!({ "type": "decision", "req_id": req_id, "decision": view }));
        Ok(())
    }

    pub(super) fn list_tags(&self, req_id: &Value) -> anyhow::Result<()> {
        let tags = self.app.db.list_tags_with_uses()?;
        self.send(json!({ "type": "tags", "req_id": req_id, "tags": tags }));
        Ok(())
    }

    pub(super) fn upsert_tag(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let name = Self::str_field(req, "name")?;
        let checked = decisions::checked_tags(std::slice::from_ref(&name.to_string()))?;
        let name = checked
            .first()
            .ok_or_else(|| anyhow::anyhow!("'name' is required"))?;
        let tag = self.app.db.upsert_tag(
            name,
            decisions::checked_tag_description(req.get("description").and_then(|v| v.as_str()))?,
            decisions::checked_color(req.get("color").and_then(|v| v.as_str()))?,
            "user",
        )?;
        self.send(json!({ "type": "tag", "req_id": req_id, "tag": tag }));
        Ok(())
    }

    /// The owner may retire any tag, however much history it files — the cap
    /// in the MCP tool exists to stop a bot unfiling their rulings, not them.
    pub(super) fn retire_tag(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let tag = self.app.db.retire_tag(
            Self::str_field(req, "name")?,
            req.get("into").and_then(|v| v.as_str()),
        )?;
        self.send(json!({ "type": "tag", "req_id": req_id, "tag": tag }));
        Ok(())
    }

    /// Rename a tag. Every decision keeps it, because they reference the id.
    pub(super) fn rename_tag(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let name = Self::str_field(req, "name")?.to_lowercase();
        let checked = decisions::checked_tags(std::slice::from_ref(
            &Self::str_field(req, "to")?.to_string(),
        ))?;
        let to = checked
            .first()
            .ok_or_else(|| anyhow::anyhow!("'to' is required"))?;
        if self.app.db.get_tag(&name)?.is_none() {
            return Err(decisions::not_found(format!("no tag named '{name}'")));
        }
        // Renaming onto a name already in use would be a merge, and merging is
        // `retire_tag with into` — which keeps the history rather than guessing.
        if self.app.db.get_tag(to)?.is_some() {
            return Err(decisions::conflict(format!(
                "a tag named '{to}' already exists; use retire_tag with into to merge"
            )));
        }
        let tag = self.app.db.rename_tag(&name, to)?;
        self.send(json!({ "type": "tag", "req_id": req_id, "tag": tag }));
        Ok(())
    }

    /// Delete a tag outright, unfiling every decision that carried it. Retiring
    /// is the reversible one; this is the owner saying the category was wrong.
    pub(super) fn delete_tag(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let name = Self::str_field(req, "name")?;
        if !self.app.db.delete_tag(name)? {
            return Err(decisions::not_found(format!("no tag named '{name}'")));
        }
        self.send(json!({ "type": "ok", "req_id": req_id }));
        Ok(())
    }

    pub(super) fn set_project_lead(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let project_id = Self::str_field(req, "project_id")?;
        let bot_id = req.get("bot_id").and_then(|v| v.as_str());
        if let Some(bot_id) = bot_id {
            let bot = self
                .app
                .db
                .get_live_bot(bot_id)?
                .ok_or_else(|| anyhow::anyhow!("no such bot"))?;
            if bot.project_id != project_id {
                anyhow::bail!("a project's lead must be a bot in that project");
            }
        }
        self.app.db.set_project_lead(project_id, bot_id)?;
        let project = self
            .app
            .db
            .get_project(project_id)?
            .ok_or_else(|| anyhow::anyhow!("no such project"))?;
        self.app.events.push(crate::events::Push::ProjectUpdated {
            project: project.clone(),
        });
        self.send(json!({
            "type": "project", "req_id": req_id, "project": super::project_view(&project)
        }));
        Ok(())
    }
}
