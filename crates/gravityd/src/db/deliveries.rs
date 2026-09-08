//! Durable delivery queue and inboxes.

use bus::*;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension, Row};

use super::{parse_ts, ts, Db};

impl Db {
    // ---- deliveries ----

    fn delivery_from_row(r: &Row<'_>) -> rusqlite::Result<Delivery> {
        let state: String = r.get(3)?;
        Ok(Delivery {
            id: r.get(0)?,
            message_id: r.get(1)?,
            bot_id: r.get(2)?,
            state: DeliveryState::parse(&state).unwrap_or(DeliveryState::Failed),
            attempt_count: r.get(4)?,
            next_attempt_at: parse_ts(&r.get::<_, String>(5)?),
            last_error: r.get(6)?,
            created_at: parse_ts(&r.get::<_, String>(7)?),
        })
    }

    const DELIVERY_COLS: &'static str =
        "id, message_id, bot_id, state, attempt_count, next_attempt_at, last_error, created_at";

    /// Enqueue a delivery. Idempotent on `idempotency_key`; returns the
    /// existing delivery when the key was seen before.
    pub fn enqueue_delivery(
        &self,
        message_id: &str,
        bot_id: &str,
        idempotency_key: &str,
    ) -> anyhow::Result<Delivery> {
        let conn = self.lock();
        let existing = conn
            .query_row(
                &format!(
                    "SELECT {} FROM delivery WHERE idempotency_key = ?1",
                    Self::DELIVERY_COLS
                ),
                params![idempotency_key],
                Self::delivery_from_row,
            )
            .optional()?;
        if let Some(d) = existing {
            return Ok(d);
        }
        let d = Delivery {
            id: new_id(),
            message_id: message_id.to_string(),
            bot_id: bot_id.to_string(),
            state: DeliveryState::Queued,
            attempt_count: 0,
            next_attempt_at: now(),
            last_error: None,
            created_at: now(),
        };
        conn.execute(
            "INSERT INTO delivery(id, message_id, bot_id, state, attempt_count, next_attempt_at, idempotency_key, created_at)
             VALUES (?1, ?2, ?3, 'queued', 0, ?4, ?5, ?6)",
            params![d.id, d.message_id, d.bot_id, ts(d.next_attempt_at), idempotency_key, ts(d.created_at)],
        )?;
        Ok(d)
    }

    /// Atomically claim due queued deliveries by moving them to `leased`.
    pub fn lease_due_deliveries(
        &self,
        lease_seconds: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<Delivery>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let now_s = ts(now());
        let lease_until = ts(now() + Duration::seconds(lease_seconds));
        let due: Vec<Delivery> = {
            let mut stmt = tx.prepare(&format!(
                "SELECT {} FROM delivery WHERE state = 'queued' AND next_attempt_at <= ?1
                 ORDER BY created_at LIMIT ?2",
                Self::DELIVERY_COLS
            ))?;
            let rows = stmt.query_map(params![now_s, limit], Self::delivery_from_row)?;
            rows.collect::<Result<_, _>>()?
        };
        for d in &due {
            tx.execute(
                "UPDATE delivery SET state = 'leased', lease_until = ?2, attempt_count = attempt_count + 1
                 WHERE id = ?1",
                params![d.id, lease_until],
            )?;
        }
        tx.commit()?;
        Ok(due)
    }

    /// Return expired leases to `queued` (crash recovery / stuck worker).
    pub fn recover_expired_leases(&self) -> anyhow::Result<u64> {
        let conn = self.lock();
        let n = conn.execute(
            "UPDATE delivery SET state = 'queued', lease_until = NULL
             WHERE state = 'leased' AND lease_until < ?1",
            params![ts(now())],
        )?;
        Ok(n as u64)
    }

    pub fn mark_delivered(&self, delivery_id: &str) -> anyhow::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE delivery SET state = 'delivered', lease_until = NULL, last_error = NULL WHERE id = ?1",
            params![delivery_id],
        )?;
        conn.execute(
            "INSERT INTO inbox(delivery_id, bot_id, read)
             SELECT id, bot_id, 0 FROM delivery WHERE id = ?1
             ON CONFLICT(delivery_id) DO NOTHING",
            params![delivery_id],
        )?;
        Ok(())
    }

    pub fn mark_delivery_retry(
        &self,
        delivery_id: &str,
        error: &str,
        next_attempt_at: DateTime<Utc>,
        max_attempts: i64,
    ) -> anyhow::Result<DeliveryState> {
        let conn = self.lock();
        let attempts: i64 = conn.query_row(
            "SELECT attempt_count FROM delivery WHERE id = ?1",
            params![delivery_id],
            |r| r.get(0),
        )?;
        let state = if attempts >= max_attempts {
            DeliveryState::Failed
        } else {
            DeliveryState::Queued
        };
        conn.execute(
            "UPDATE delivery SET state = ?2, lease_until = NULL, last_error = ?3, next_attempt_at = ?4
             WHERE id = ?1",
            params![delivery_id, state.as_str(), error, ts(next_attempt_at)],
        )?;
        Ok(state)
    }

    /// Requeue a failed delivery for a fresh round of attempts.
    pub fn retry_failed_delivery(&self, delivery_id: &str) -> anyhow::Result<bool> {
        let conn = self.lock();
        let n = conn.execute(
            "UPDATE delivery SET state = 'queued', attempt_count = 0, last_error = NULL, next_attempt_at = ?2
             WHERE id = ?1 AND state = 'failed'",
            params![delivery_id, ts(now())],
        )?;
        Ok(n > 0)
    }

    pub fn get_delivery(&self, id: &str) -> anyhow::Result<Option<Delivery>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                &format!("SELECT {} FROM delivery WHERE id = ?1", Self::DELIVERY_COLS),
                params![id],
                Self::delivery_from_row,
            )
            .optional()?)
    }

    pub fn list_deliveries(
        &self,
        bot_id: Option<&str>,
        state: Option<DeliveryState>,
    ) -> anyhow::Result<Vec<Delivery>> {
        let conn = self.lock();
        let mut sql = format!("SELECT {} FROM delivery WHERE 1=1", Self::DELIVERY_COLS);
        if bot_id.is_some() {
            sql.push_str(" AND bot_id = ?1");
        }
        if let Some(s) = state {
            sql.push_str(&format!(" AND state = '{}'", s.as_str()));
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT 200");
        let mut stmt = conn.prepare(&sql)?;
        let rows = match bot_id {
            Some(b) => stmt.query_map(params![b], Self::delivery_from_row)?,
            None => stmt.query_map([], Self::delivery_from_row)?,
        };
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn delivery_backlog(&self) -> anyhow::Result<i64> {
        let conn = self.lock();
        Ok(conn.query_row(
            "SELECT count(*) FROM delivery WHERE state IN ('queued', 'leased', 'failed')",
            [],
            |r| r.get(0),
        )?)
    }

    /// Unread inbox for a bot; marks returned entries read and their
    /// deliveries acknowledged (application-level consumption).
    pub fn consume_inbox(&self, bot_id: &str) -> anyhow::Result<Vec<Message>> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let msgs: Vec<Message> = {
            let mut stmt = tx.prepare(&format!(
                "SELECT {} FROM message WHERE id IN (
                    SELECT d.message_id FROM inbox i JOIN delivery d ON d.id = i.delivery_id
                    WHERE i.bot_id = ?1 AND i.read = 0
                 ) ORDER BY num",
                Self::MSG_COLS
            ))?;
            let rows = stmt.query_map(params![bot_id], Self::message_from_row)?;
            rows.collect::<Result<_, _>>()?
        };
        tx.execute(
            "UPDATE delivery SET state = 'acknowledged'
             WHERE id IN (SELECT delivery_id FROM inbox WHERE bot_id = ?1 AND read = 0)
               AND state = 'delivered'",
            params![bot_id],
        )?;
        tx.execute(
            "UPDATE inbox SET read = 1 WHERE bot_id = ?1 AND read = 0",
            params![bot_id],
        )?;
        tx.commit()?;
        Ok(msgs)
    }

    /// Return a leased delivery to the queue without consuming a retry
    /// attempt (bot not ready: stopped, or inbox socket unknown).
    pub fn defer_delivery(
        &self,
        delivery_id: &str,
        next_attempt_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        self.lock().execute(
            "UPDATE delivery SET state = 'queued', lease_until = NULL,
             attempt_count = MAX(attempt_count - 1, 0), next_attempt_at = ?2
             WHERE id = ?1 AND state = 'leased'",
            params![delivery_id, ts(next_attempt_at)],
        )?;
        Ok(())
    }

    pub fn ack_delivery(&self, delivery_id: &str) -> anyhow::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE delivery SET state = 'acknowledged' WHERE id = ?1 AND state = 'delivered'",
            params![delivery_id],
        )?;
        conn.execute(
            "UPDATE inbox SET read = 1 WHERE delivery_id = ?1",
            params![delivery_id],
        )?;
        Ok(())
    }

    /// Fail an undelivered message whose work was cancelled. A delivered one
    /// is left alone: the bot has already seen it.
    pub fn cancel_delivery(&self, delivery_id: &str, reason: &str) -> anyhow::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE delivery SET state = 'failed', lease_until = NULL, last_error = ?2
             WHERE id = ?1 AND state IN ('queued', 'leased')",
            params![delivery_id, reason],
        )?;
        Ok(())
    }

    pub fn unread_count(&self, bot_id: &str) -> anyhow::Result<i64> {
        let conn = self.lock();
        Ok(conn.query_row(
            "SELECT count(*) FROM inbox WHERE bot_id = ?1 AND read = 0",
            params![bot_id],
            |r| r.get(0),
        )?)
    }
}
