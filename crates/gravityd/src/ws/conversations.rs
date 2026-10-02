//! Conversations between a project's bots: which pairs have talked, and what
//! they said. See "Conversations" in `docs/protocol.md`.

use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::app::AppState;
use crate::db::agent_talk::AgentMessage;
use crate::db::Db;

use super::Conn;

const DEFAULT_PAGE: usize = 50;
const MAX_PAGE: usize = 200;
/// Characters of the latest message shown in the list.
const PREVIEW_CHARS: usize = 200;

/// Who a bot is in a conversation, archived bots included: a thread outlives
/// the bots in it.
fn who(app: &AppState, ids: impl IntoIterator<Item = String>) -> Vec<Value> {
    ids.into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|id| app.db.get_bot(&id).ok().flatten())
        .map(|bot| {
            let machine = bot
                .peer_id
                .as_deref()
                .and_then(|peer| app.db.get_peer(peer).ok().flatten())
                .map(|peer| Db::display_peer_name(&peer));
            json!({
                "id": bot.id, "name": Db::display_name(&bot), "avatar": bot.avatar,
                "machine": machine, "deleted": bot.deleted_at.is_some()
            })
        })
        .collect()
}

fn message_json(app: &AppState, m: &AgentMessage, preview: bool) -> Value {
    let body = if preview {
        crate::chat::truncate(&m.message.body, PREVIEW_CHARS)
    } else {
        m.message.body.clone()
    };
    let task = (m.message.kind == bus::MessageKind::Task)
        .then(|| app.db.tasks_with_origin(&m.message.id).ok())
        .flatten()
        .and_then(|tasks| tasks.into_iter().next())
        .map(|t| json!({ "id": t.id, "state": t.state }));
    json!({
        "id": m.message.id,
        "num": m.message.num,
        "from_bot_id": m.message.sender.bot_id,
        "to_bot_id": m.to_bot_id,
        "kind": m.message.kind,
        "body": body,
        "ref_message_id": m.message.ref_message_id,
        "task": task,
        "created_at": m.message.created_at.to_rfc3339(),
    })
}

impl Conn {
    /// The pairs of bots in a project that have talked, most recent first.
    pub(super) fn list_agent_conversations(
        &self,
        req_id: &Value,
        req: &Value,
    ) -> anyhow::Result<()> {
        let project_id = Self::str_field(req, "project_id")?.to_string();
        self.blocking(req_id, move |app| {
            let pairs = app.db.agent_pairs(&project_id)?;
            let mut conversations = Vec::with_capacity(pairs.len());
            for pair in &pairs {
                let last = app.db.agent_pair_last(&project_id, pair)?;
                conversations.push(json!({
                    "bot_ids": pair.bots,
                    "message_count": pair.messages,
                    "last_at": pair.last_at.to_rfc3339(),
                    "last": last.map(|m| message_json(app, &m, true)),
                }));
            }
            let bots = who(app, pairs.into_iter().flat_map(|p| p.bots));
            Ok(json!({
                "type": "agent_conversations", "project_id": project_id,
                "conversations": conversations, "bots": bots
            }))
        });
        Ok(())
    }

    /// The messages between two bots, oldest first, a page at a time
    /// (`before` is a message `num`).
    pub(super) fn list_agent_conversation(
        &self,
        req_id: &Value,
        req: &Value,
    ) -> anyhow::Result<()> {
        let project_id = Self::str_field(req, "project_id")?.to_string();
        let ids: Vec<String> = req
            .get("bot_ids")
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let [a, b] = <[String; 2]>::try_from(ids)
            .map_err(|_| anyhow::anyhow!("'bot_ids' must name two bots"))?;
        let before = req.get("before").and_then(Value::as_i64);
        let limit = req
            .get("limit")
            .and_then(Value::as_u64)
            .map_or(DEFAULT_PAGE, |n| (n as usize).clamp(1, MAX_PAGE));
        self.blocking(req_id, move |app| {
            let (messages, has_more) = app.db.agent_thread(&project_id, &a, &b, before, limit)?;
            Ok(json!({
                "type": "agent_conversation", "project_id": project_id,
                "bot_ids": [a.clone(), b.clone()],
                "messages": messages.iter().map(|m| message_json(app, m, false)).collect::<Vec<_>>(),
                "has_more": has_more,
                "bots": who(app, [a, b]),
            }))
        });
        Ok(())
    }
}
