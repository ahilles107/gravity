//! Request routing: the capability gate and the type-to-handler table.
//!
//! Kept apart from the connection plumbing because it is the one list that
//! grows with every new request, and because the capability rule has to be
//! readable in one screen to stay auditable.

use bus::Capability;
use serde_json::{json, Value};

use super::Conn;

/// Requests that only read. Everything else requires `control`.
const READ_ONLY: &[&str] = &[
    "list_projects",
    "list_bots",
    "list_messages",
    "list_conversations",
    "list_routines",
    "list_routine_runs",
    "list_deliveries",
    "list_devices",
    "search",
    "diagnostics",
    "get_config",
    "attach",
    "detach",
    "list_bot_revisions",
    "list_bot_activity",
    "list_decisions",
    "get_decision",
    "list_tags",
    "count_pending_decisions",
    "list_peers",
    "list_peer_bots",
    "list_peer_projects",
    "watch_browser",
    "unwatch_browser",
    "list_browser_activity",
    "list_bot_commands",
    "list_agent_conversations",
    "list_agent_conversation",
    "list_chat",
    "get_chat_step",
    "get_chat_image",
    "list_artifacts",
    "read_file",
    "list_permissions",
    "list_tasks",
    "get_task",
];

/// Requests that exercise the owner's ruling authority.
///
/// `control` is running the fleet; this is answering for the owner, and the
/// registry is only worth anything if those are different grants. Withdrawing
/// and commenting stay under `control` because a bot may do both.
const APPROVE_ONLY: &[&str] = &[
    "answer_decision",
    "unanswer_decision",
    "hold_decision",
    "resume_decision",
    "confirm_decision",
    "reopen_decision",
    "update_decision",
    "delete_decision",
    "publish_decisions",
];

/// Capability required for each request type.
pub(super) fn required_cap(kind: &str) -> Capability {
    if READ_ONLY.contains(&kind) {
        Capability::Read
    } else if APPROVE_ONLY.contains(&kind) {
        Capability::Approve
    } else {
        Capability::Control
    }
}

impl Conn {
    pub(super) fn dispatch(&mut self, req: &Value) {
        let req_id = req.get("req_id").cloned().unwrap_or(Value::Null);
        let kind = req.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let cap = required_cap(kind);
        if !self.caps.contains(&cap) {
            self.reply_err(
                &req_id,
                "forbidden",
                &format!("'{kind}' requires the {} capability", cap.as_str()),
            );
            return;
        }
        let result = match kind {
            "list_projects" => self.list_projects(&req_id),
            "create_project" => self.create_project(&req_id, req),
            "update_project" => self.update_project(&req_id, req),
            "delete_project" => self.delete_project(&req_id, req),
            "list_bots" => self.list_bots(&req_id, req),
            "list_bot_activity" => self.list_bot_activity(&req_id, req),
            "create_bot" => self.create_bot(&req_id, req),
            "update_bot" => self.update_bot(&req_id, req),
            "set_bot_runtime" => self.set_bot_runtime(&req_id, req),
            "set_bot_user_chrome" => self.set_bot_user_chrome(&req_id, req),
            "restart_bot" => self.restart_bot(&req_id, req),
            "clear_bot_session" => self.clear_bot_session(&req_id, req),
            "watch_browser" => self.watch_browser(&req_id, req),
            "unwatch_browser" => self.unwatch_browser(&req_id),
            "list_browser_activity" => self.list_browser_activity(&req_id, req),
            "list_bot_commands" => self.list_bot_commands(&req_id, req),
            "list_agent_conversations" => self.list_agent_conversations(&req_id, req),
            "list_agent_conversation" => self.list_agent_conversation(&req_id, req),
            "delete_bot" => self.delete_bot(&req_id, req),
            "list_bot_revisions" => self.list_bot_revisions(&req_id, req),
            "revert_bot_revision" => self.revert_bot_revision(&req_id, req),
            "attach" => self.attach(&req_id, req),
            "detach" => self.detach(&req_id, req),
            "input" => self.input(req),
            "resize" => self.resize(req),
            "send_user_message" => self.send_user_message(&req_id, req),
            "list_messages" => self.list_messages(&req_id, req),
            "list_conversations" => self.list_conversations(&req_id, req),
            "list_routines" => self.list_routines(&req_id, req),
            "create_routine" => self.create_routine(&req_id, req),
            "set_routine_enabled" => self.set_routine_enabled(&req_id, req),
            "run_routine_now" => self.run_routine_now(&req_id, req),
            "cancel_routine_run" => self.cancel_routine_run(&req_id, req),
            "emit_signal" => self.emit_signal(&req_id, req),
            "list_routine_runs" => self.list_routine_runs(&req_id, req),
            "list_deliveries" => self.list_deliveries(&req_id, req),
            "retry_delivery" => self.retry_delivery(&req_id, req),
            "search" => self.search(&req_id, req),
            "diagnostics" => self.diagnostics(&req_id),
            "get_config" => self.get_config(&req_id),
            "set_config" => self.set_config(&req_id, req),
            "list_devices" => self.list_devices(&req_id),
            "create_device" => self.create_device(&req_id, req),
            "revoke_device" => self.revoke_device(&req_id, req),
            "list_decisions" => self.list_decisions(&req_id, req),
            "get_decision" => self.get_decision(&req_id, req),
            "count_pending_decisions" => self.count_pending_decisions(&req_id),
            "answer_decision" => self.answer_decision(&req_id, req),
            "unanswer_decision" => self.unanswer_decision(&req_id, req),
            "hold_decision" => self.hold_decision(&req_id, req),
            "resume_decision" => self.resume_decision(&req_id, req),
            "confirm_decision" => self.confirm_decision(&req_id, req),
            "withdraw_decision" => self.withdraw_decision(&req_id, req),
            "reopen_decision" => self.reopen_decision(&req_id, req),
            "comment_decision" => self.comment_decision(&req_id, req),
            "update_decision" => self.update_decision(&req_id, req),
            "delete_decision" => self.delete_decision(&req_id, req),
            "publish_decisions" => self.publish_decisions(&req_id, req),
            "set_decision_tags" => self.set_decision_tags(&req_id, req),
            "list_tags" => self.list_tags(&req_id),
            "upsert_tag" => self.upsert_tag(&req_id, req),
            "retire_tag" => self.retire_tag(&req_id, req),
            "rename_tag" => self.rename_tag(&req_id, req),
            "delete_tag" => self.delete_tag(&req_id, req),
            "set_project_lead" => self.set_project_lead(&req_id, req),
            "list_peers" => self.list_peers(&req_id),
            "create_peer_invite" => self.create_peer_invite(&req_id, req),
            "add_peer" => self.add_peer(&req_id, req),
            "revoke_peer" => self.revoke_peer(&req_id, req),
            "list_peer_bots" => self.list_peer_bots(&req_id, req),
            "link_peer_bot" => self.link_peer_bot(&req_id, req),
            "list_peer_projects" => self.list_peer_projects(&req_id, req),
            "link_project" => self.link_project(&req_id, req),
            "unlink_project" => self.unlink_project(&req_id, req),
            "list_chat" => self.list_chat(&req_id, req),
            "get_chat_step" => self.get_chat_step(&req_id, req),
            "get_chat_image" => self.get_chat_image(&req_id, req),
            "list_artifacts" => self.list_artifacts(&req_id, req),
            "read_file" => self.read_file(&req_id, req),
            "list_permissions" => self.list_permissions(&req_id, req),
            "list_tasks" => self.list_tasks(&req_id, req),
            "get_task" => self.get_task(&req_id, req),
            "write_artifact" => self.write_artifact(&req_id, req),
            "answer_permission" => self.answer_permission(&req_id, req),
            other => {
                self.reply_err(
                    &req_id,
                    "invalid_request",
                    &format!("unknown type: {other}"),
                );
                Ok(())
            }
        };
        if let Err(e) = result {
            // A refusal the caller can act on — not found, forbidden, a state
            // conflict — is not an internal error, and a client that sees
            // `internal` has nothing useful to show the owner.
            let code = crate::decisions::error_code(&e).unwrap_or("internal");
            self.reply_err(&req_id, code, &e.to_string());
        }
    }

    pub(super) fn send(&self, v: Value) {
        let _ = self.out.send(v);
    }

    pub(super) fn reply_err(&self, req_id: &Value, code: &str, message: &str) {
        self.send(json!({
            "type": "error", "req_id": req_id, "code": code, "message": message
        }));
    }

    pub(super) fn str_field<'a>(req: &'a Value, field: &str) -> anyhow::Result<&'a str> {
        req.get(field)
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'{field}' is required"))
    }

    pub(super) fn bot_json(&self, bot: &bus::Bot) -> Value {
        super::views::bot_view(&self.app, bot)
    }
}
