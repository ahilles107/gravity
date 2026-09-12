//! Control-plane requests for the decision registry.
//!
//! Thin by design: parse, call `crate::decisions`, serialise. The rules live in
//! the service so a bot and the owner cannot be told different things about the
//! same record.

use bus::{DecisionState, Priority};
use serde_json::{json, Value};

use crate::db::{Actor, DecisionFilter};
use crate::decisions::{self, Detail};

use super::Conn;

/// How far ahead a deadline counts as "due soon" for the badge.
const DUE_SOON_HOURS: i64 = 24;

impl Conn {
    /// The owner, named by the credential they authenticated with.
    pub(super) fn owner(&self) -> Actor<'_> {
        match &self.device_id {
            Some(id) => Actor::Device(id),
            None => Actor::User,
        }
    }

    pub(super) fn list_decisions(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let states: Vec<DecisionState> = match req.get("state").and_then(|v| v.as_str()) {
            Some("settled") => vec![DecisionState::Settled],
            Some("held") => vec![DecisionState::Held],
            Some("withdrawn") => vec![DecisionState::Withdrawn],
            Some("all") => Vec::new(),
            // Default is the working list: what still wants an answer.
            _ => vec![DecisionState::Open, DecisionState::Answered],
        };
        let decisions_found = self.app.db.list_decisions(&DecisionFilter {
            project_id: req.get("project_id").and_then(|v| v.as_str()),
            states: &states,
            tag: req.get("tag").and_then(|v| v.as_str()),
            bot_id: req.get("bot_id").and_then(|v| v.as_str()),
            query: req.get("query").and_then(|v| v.as_str()),
            before: req.get("before").and_then(|v| v.as_str()),
            limit: req.get("limit").and_then(|v| v.as_i64()).unwrap_or(100),
        })?;
        let views = decisions::decision_views(&self.app.db, &decisions_found)?;
        self.send(json!({ "type": "decisions", "req_id": req_id, "decisions": views }));
        Ok(())
    }

    pub(super) fn get_decision(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let decision = decisions::load(&self.app, Self::str_field(req, "decision_id")?)?;
        let view = decisions::decision_view(&self.app.db, &decision, Detail::Full)?;
        self.reply_decision(req_id, &view)
    }

    pub(super) fn count_pending_decisions(&self, req_id: &Value) -> anyhow::Result<()> {
        let counts = self.app.db.count_pending_decisions(DUE_SOON_HOURS)?;
        self.send(json!({
            "type": "pending_decisions", "req_id": req_id, "counts": counts
        }));
        Ok(())
    }

    pub(super) fn answer_decision(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let view = decisions::answer(
            &self.app,
            &self.owner(),
            Self::str_field(req, "decision_id")?,
            req.get("ruling_option").and_then(|v| v.as_str()),
            Self::str_field(req, "ruling_text")?,
            req.get("ruling_reason").and_then(|v| v.as_str()),
        )?;
        self.reply_decision(req_id, &view)
    }

    pub(super) fn unanswer_decision(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let view = decisions::unanswer(
            &self.app,
            &self.owner(),
            Self::str_field(req, "decision_id")?,
        )?;
        self.reply_decision(req_id, &view)
    }

    pub(super) fn hold_decision(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let view = decisions::hold(
            &self.app,
            &self.owner(),
            Self::str_field(req, "decision_id")?,
            req.get("until").and_then(|v| v.as_str()),
            req.get("comment").and_then(|v| v.as_str()),
        )?;
        self.reply_decision(req_id, &view)
    }

    pub(super) fn resume_decision(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let view = decisions::resume(
            &self.app,
            &self.owner(),
            Self::str_field(req, "decision_id")?,
        )?;
        self.reply_decision(req_id, &view)
    }

    pub(super) fn confirm_decision(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let view = decisions::confirm(
            &self.app,
            &self.owner(),
            Self::str_field(req, "decision_id")?,
        )?;
        self.reply_decision(req_id, &view)
    }

    pub(super) fn withdraw_decision(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let view = decisions::withdraw(
            &self.app,
            &self.owner(),
            Self::str_field(req, "decision_id")?,
            req.get("reason").and_then(|v| v.as_str()).unwrap_or(""),
        )?;
        self.reply_decision(req_id, &view)
    }

    pub(super) fn reopen_decision(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let view = decisions::reopen(
            &self.app,
            &self.owner(),
            Self::str_field(req, "decision_id")?,
            req.get("title").and_then(|v| v.as_str()),
            req.get("body").and_then(|v| v.as_str()),
        )?;
        self.reply_decision(req_id, &view)
    }

    pub(super) fn comment_decision(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let comment = decisions::comment(
            &self.app,
            &self.owner(),
            Self::str_field(req, "decision_id")?,
            Self::str_field(req, "body")?,
        )?;
        self.send(json!({
            "type": "decision_comment", "req_id": req_id, "comment": comment
        }));
        Ok(())
    }

    pub(super) fn update_decision(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let decision_id = Self::str_field(req, "decision_id")?;
        let options: Option<Vec<bus::DecisionOption>> = match req.get("options") {
            Some(value) if !value.is_null() => Some(serde_json::from_value(value.clone())?),
            _ => None,
        };
        let view = decisions::update(
            &self.app,
            &self.owner(),
            decision_id,
            &decisions::Patch {
                title: req.get("title").and_then(|v| v.as_str()),
                body: req.get("body").and_then(|v| v.as_str()),
                options: options.as_deref(),
                recommendation: req.get("recommendation").and_then(|v| v.as_str()),
                priority: req
                    .get("priority")
                    .and_then(|v| v.as_str())
                    .and_then(Priority::parse),
                // Present-but-null clears the deadline; absent leaves it.
                deadline_at: req
                    .get("deadline_at")
                    .map(|v| v.as_str().filter(|s| !s.is_empty())),
                ruling_option: req.get("ruling_option").and_then(|v| v.as_str()),
                ruling_text: req.get("ruling_text").and_then(|v| v.as_str()),
                ruling_reason: req.get("ruling_reason").and_then(|v| v.as_str()),
            },
        )?;
        if req.get("renotify").and_then(|v| v.as_bool()) == Some(true) {
            self.app.db.clear_notifications(decision_id)?;
            decisions::publish(&self.app, decision_id, None)?;
        }
        self.reply_decision(req_id, &view)
    }

    pub(super) fn delete_decision(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        decisions::delete(
            &self.app,
            &self.owner(),
            Self::str_field(req, "decision_id")?,
        )?;
        self.send(json!({ "type": "ok", "req_id": req_id }));
        Ok(())
    }

    fn reply_decision(&self, req_id: &Value, view: &bus::DecisionView) -> anyhow::Result<()> {
        self.send(json!({ "type": "decision", "req_id": req_id, "decision": view }));
        Ok(())
    }
}
