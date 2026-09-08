//! Named signals: the one extension point for non-time triggers. Adding a
//! producer means writing rows here, not adding a `Trigger` variant.

use bus::*;
use rusqlite::{params, OptionalExtension, Row};

use super::{parse_ts, ts, Db};

pub struct NewSignal<'a> {
    pub name: &'a str,
    pub source: SignalSource,
    pub project_id: &'a str,
    pub from_bot_id: Option<&'a str>,
    pub payload: serde_json::Value,
    pub origin_chain: String,
    pub hop_count: i64,
}

impl Db {
    fn signal_from_row(r: &Row<'_>) -> rusqlite::Result<Signal> {
        let source: String = r.get(2)?;
        let payload: String = r.get(5)?;
        Ok(Signal {
            id: r.get(0)?,
            name: r.get(1)?,
            source: SignalSource::parse(&source).unwrap_or(SignalSource::Manual),
            project_id: r.get(3)?,
            from_bot_id: r.get(4)?,
            payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
            origin_chain: r.get(6)?,
            hop_count: r.get(7)?,
            emitted_at: parse_ts(&r.get::<_, String>(8)?),
        })
    }

    const SIGNAL_COLS: &'static str = "id, name, source, project_id, from_bot_id, payload_json, \
         origin_chain, hop_count, emitted_at";

    pub fn emit_signal(&self, new: NewSignal<'_>) -> anyhow::Result<Signal> {
        let signal = Signal {
            id: new_id(),
            name: new.name.to_string(),
            source: new.source,
            project_id: new.project_id.to_string(),
            from_bot_id: new.from_bot_id.map(str::to_string),
            payload: new.payload,
            origin_chain: new.origin_chain,
            hop_count: new.hop_count,
            emitted_at: now(),
        };
        self.lock().execute(
            "INSERT INTO signal(id, name, source, project_id, from_bot_id, payload_json,
                                origin_chain, hop_count, emitted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                signal.id,
                signal.name,
                signal.source.as_str(),
                signal.project_id,
                signal.from_bot_id,
                serde_json::to_string(&signal.payload)?,
                signal.origin_chain,
                signal.hop_count,
                ts(signal.emitted_at)
            ],
        )?;
        Ok(signal)
    }

    pub fn get_signal(&self, id: &str) -> anyhow::Result<Option<Signal>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                &format!("SELECT {} FROM signal WHERE id = ?1", Self::SIGNAL_COLS),
                params![id],
                Self::signal_from_row,
            )
            .optional()?)
    }

    pub fn list_signals(&self, project_id: &str, limit: i64) -> anyhow::Result<Vec<Signal>> {
        let limit = limit.clamp(1, 200);
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM signal WHERE project_id = ?1 ORDER BY emitted_at DESC LIMIT ?2",
            Self::SIGNAL_COLS
        ))?;
        let rows = stmt.query_map(params![project_id, limit], Self::signal_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The signal that caused a bot's current work, used to extend the chain
    /// across it. A bot cannot name the occurrence it is inside, so this is
    /// inferred; getting it wrong only undercounts hops, and the exact cycle
    /// guard is `origin_chain` membership.
    pub fn causing_signal_for_bot(&self, bot_id: &str) -> anyhow::Result<Option<Signal>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                &format!(
                    "SELECT {} FROM signal s
                       JOIN routine_run r ON r.signal_id = s.id
                       JOIN routine rt ON rt.id = r.routine_id
                      WHERE rt.bot_id = ?1 AND r.state = 'running'
                      ORDER BY r.started_at DESC LIMIT 1",
                    Self::SIGNAL_COLS
                        .split(',')
                        .map(|c| format!("s.{}", c.trim()))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                params![bot_id],
                Self::signal_from_row,
            )
            .optional()?)
    }

    /// Only signals no surviving run points at.
    pub fn prune_signals(&self, cutoff: chrono::DateTime<chrono::Utc>) -> anyhow::Result<usize> {
        let conn = self.lock();
        Ok(conn.execute(
            "DELETE FROM signal WHERE emitted_at < ?1
               AND id NOT IN (SELECT signal_id FROM routine_run WHERE signal_id IS NOT NULL)",
            params![ts(cutoff)],
        )?)
    }
}
