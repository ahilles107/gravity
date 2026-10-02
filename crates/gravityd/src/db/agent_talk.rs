//! Conversations between bots. A message from one bot to another is stored in
//! the recipient's DM with the sender recorded, so a pair's conversation is
//! the two DMs read together.

use bus::Message;
use chrono::{DateTime, Utc};
use rusqlite::params;

use super::{parse_ts, Db};

/// Two bots that have talked, and how much.
#[derive(Debug, Clone)]
pub struct AgentPair {
    /// The two bot ids, in a fixed order.
    pub bots: [String; 2],
    pub messages: i64,
    pub last_num: i64,
    pub last_at: DateTime<Utc>,
}

/// A message between two bots, with the bot it went to.
#[derive(Debug, Clone)]
pub struct AgentMessage {
    pub message: Message,
    pub to_bot_id: String,
}

/// Bot-to-bot messages in a project: a bot's DM, written by another bot.
const TALK: &str = "FROM message m JOIN conversation c ON c.id = m.conversation_id
     WHERE c.project_id = ?1 AND c.kind = 'dm' AND m.sender_kind = 'bot'
       AND m.sender_bot_id IS NOT NULL AND c.bot_id IS NOT NULL
       AND m.sender_bot_id != c.bot_id";

impl Db {
    /// Every pair of bots in a project that has exchanged messages, the most
    /// recently active first.
    pub fn agent_pairs(&self, project_id: &str) -> anyhow::Result<Vec<AgentPair>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT min(m.sender_bot_id, c.bot_id) AS lo, max(m.sender_bot_id, c.bot_id) AS hi,
                    count(*), max(m.num), max(m.created_at)
             {TALK} GROUP BY lo, hi ORDER BY max(m.num) DESC"
        ))?;
        let rows = stmt.query_map(params![project_id], |r| {
            Ok(AgentPair {
                bots: [r.get(0)?, r.get(1)?],
                messages: r.get(2)?,
                last_num: r.get(3)?,
                last_at: parse_ts(&r.get::<_, String>(4)?),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Up to `limit` messages between two bots before message number
    /// `before` (or the newest), oldest first, and whether older ones exist.
    pub fn agent_thread(
        &self,
        project_id: &str,
        a: &str,
        b: &str,
        before: Option<i64>,
        limit: usize,
    ) -> anyhow::Result<(Vec<AgentMessage>, bool)> {
        let cols = Self::MSG_COLS
            .split(", ")
            .map(|c| format!("m.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {cols}, c.bot_id {TALK}
               AND ((m.sender_bot_id = ?2 AND c.bot_id = ?3)
                 OR (m.sender_bot_id = ?3 AND c.bot_id = ?2))
               AND (?4 IS NULL OR m.num < ?4)
             ORDER BY m.num DESC LIMIT ?5"
        ))?;
        let rows = stmt.query_map(params![project_id, a, b, before, limit as i64 + 1], |r| {
            Ok(AgentMessage {
                message: Self::message_from_row(r)?,
                to_bot_id: r.get(11)?,
            })
        })?;
        let mut messages: Vec<AgentMessage> = rows.collect::<Result<_, _>>()?;
        let more = messages.len() > limit;
        messages.truncate(limit);
        messages.reverse();
        Ok((messages, more))
    }

    /// The newest message of a pair, for the list's preview line.
    pub fn agent_pair_last(
        &self,
        project_id: &str,
        pair: &AgentPair,
    ) -> anyhow::Result<Option<AgentMessage>> {
        let (mut last, _) = self.agent_thread(
            project_id,
            &pair.bots[0],
            &pair.bots[1],
            Some(pair.last_num + 1),
            1,
        )?;
        Ok(last.pop())
    }
}
