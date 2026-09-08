//! Conversations and messages.

use bus::*;
use rusqlite::{params, OptionalExtension, Row};

use super::{parse_ts, ts, Db};

impl Db {
    // ---- conversations ----

    fn conversation_from_row(r: &Row<'_>) -> rusqlite::Result<Conversation> {
        Ok(Conversation {
            id: r.get(0)?,
            project_id: r.get(1)?,
            bot_id: r.get(2)?,
            title: r.get(3)?,
        })
    }

    pub fn list_conversations(
        &self,
        project_id: Option<&str>,
    ) -> anyhow::Result<Vec<Conversation>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT id, project_id, bot_id, title FROM conversation {} ORDER BY title",
            if project_id.is_some() {
                "WHERE project_id = ?1"
            } else {
                ""
            }
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = match project_id {
            Some(pid) => stmt.query_map(params![pid], Self::conversation_from_row)?,
            None => stmt.query_map([], Self::conversation_from_row)?,
        };
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn dm_conversation(&self, bot_id: &str) -> anyhow::Result<Option<Conversation>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                "SELECT id, project_id, bot_id, title
                 FROM conversation WHERE bot_id = ?1",
                params![bot_id],
                Self::conversation_from_row,
            )
            .optional()?)
    }

    // ---- messages ----

    pub(super) fn message_from_row(r: &Row<'_>) -> rusqlite::Result<Message> {
        let sender_kind: String = r.get(3)?;
        let kind: String = r.get(6)?;
        Ok(Message {
            id: r.get(0)?,
            num: r.get(1)?,
            conversation_id: r.get(2)?,
            sender: Sender {
                kind: match sender_kind.as_str() {
                    "bot" => SenderKind::Bot,
                    "routine" => SenderKind::Routine,
                    _ => SenderKind::User,
                },
                bot_id: r.get(4)?,
                name: r.get(5)?,
            },
            kind: MessageKind::parse(&kind).unwrap_or(MessageKind::Note),
            body: r.get(7)?,
            ref_message_id: r.get(8)?,
            created_at: parse_ts(&r.get::<_, String>(9)?),
        })
    }

    pub(super) const MSG_COLS: &'static str = "id, num, conversation_id, sender_kind, sender_bot_id, sender_name, kind, body, ref_message_id, created_at";

    pub fn insert_message(
        &self,
        conversation_id: &str,
        sender: &Sender,
        kind: MessageKind,
        body: &str,
        ref_message_id: Option<&str>,
    ) -> anyhow::Result<Message> {
        let conn = self.lock();
        let num: i64 =
            conn.query_row("SELECT COALESCE(MAX(num), 0) + 1 FROM message", [], |r| {
                r.get(0)
            })?;
        let msg = Message {
            id: new_id(),
            num,
            conversation_id: conversation_id.to_string(),
            sender: sender.clone(),
            kind,
            body: body.to_string(),
            ref_message_id: ref_message_id.map(|s| s.to_string()),
            created_at: now(),
        };
        conn.execute(
            "INSERT INTO message(id, num, conversation_id, sender_kind, sender_bot_id, sender_name, kind, body, ref_message_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                msg.id,
                msg.num,
                msg.conversation_id,
                match sender.kind {
                    SenderKind::User => "user",
                    SenderKind::Bot => "bot",
                    SenderKind::Routine => "routine",
                },
                sender.bot_id,
                sender.name,
                kind.as_str(),
                msg.body,
                msg.ref_message_id,
                ts(msg.created_at)
            ],
        )?;
        Ok(msg)
    }

    pub fn get_message(&self, id: &str) -> anyhow::Result<Option<Message>> {
        let conn = self.lock();
        let sql = format!("SELECT {} FROM message WHERE id = ?1", Self::MSG_COLS);
        Ok(conn
            .query_row(&sql, params![id], Self::message_from_row)
            .optional()?)
    }

    pub fn list_messages(
        &self,
        conversation_id: &str,
        before_num: Option<i64>,
        limit: i64,
    ) -> anyhow::Result<Vec<Message>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {} FROM message WHERE conversation_id = ?1 AND num < ?2 ORDER BY num DESC LIMIT ?3",
            Self::MSG_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params![conversation_id, before_num.unwrap_or(i64::MAX), limit],
            Self::message_from_row,
        )?;
        let mut msgs: Vec<Message> = rows.collect::<Result<_, _>>()?;
        msgs.reverse();
        Ok(msgs)
    }

    pub fn search_messages(&self, query: &str, limit: i64) -> anyhow::Result<Vec<Message>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {} FROM message WHERE rowid IN
               (SELECT rowid FROM message_fts WHERE message_fts MATCH ?1 LIMIT ?2)
             ORDER BY num DESC",
            Self::MSG_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![query, limit], Self::message_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
