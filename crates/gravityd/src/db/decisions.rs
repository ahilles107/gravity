//! The decision registry: rows, and the state machine over them.
//!
//! Every transition is a conditional UPDATE that names the state it expects,
//! the way `try_close_task` does. Two clients working the same batch, or a bot
//! withdrawing while the owner publishes, then resolve to one winner instead
//! of two half-applied changes — and the `bool` tells the caller whether it is
//! the one that should fire the side effect.

use bus::*;
use rusqlite::{params, OptionalExtension, Row};

use super::{parse_ts, ts, Db};

/// A decision as a bot files it. Wide enough that positional arguments would
/// be unreadable, and every field here is one a bot may set.
pub struct NewDecision<'a> {
    pub project_id: &'a str,
    pub kind: DecisionKind,
    pub title: &'a str,
    pub body: &'a str,
    pub options: &'a [DecisionOption],
    pub recommendation: Option<&'a str>,
    pub raised_by_bot_id: &'a str,
    pub on_behalf_of_bot_id: Option<&'a str>,
    pub origin_chain: &'a str,
    pub source_message_id: Option<&'a str>,
    pub source_task_id: Option<&'a str>,
    pub priority: Priority,
    pub deadline_at: Option<chrono::DateTime<chrono::Utc>>,
    pub supersedes_id: Option<&'a str>,
    /// Set only by `record_decision`, which files a ruling the owner already
    /// gave at a bot's terminal. The record is born settled and says so.
    pub settled: Option<Ruling>,
}

/// Lowercased, punctuation-free form of a title, used only to recognise the
/// same question asked twice. Deliberately lossy: "Bump Forgejo to 16?" and
/// "bump forgejo to 16" are the same ask.
pub fn normalise_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut spaced = true;
    for c in title.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_alphanumeric() {
            out.push(c);
            spaced = false;
        } else if !spaced {
            out.push(' ');
            spaced = true;
        }
    }
    out.trim_end().to_string()
}

impl Db {
    pub(super) const DECISION_COLS: &'static str =
        "id, project_id, kind, title, normalised_title, \
         body, options_json, recommendation, raised_by_bot_id, on_behalf_of_bot_id, origin_chain, \
         source_message_id, source_task_id, priority, deadline_at, deadline_notified_at, state, \
         held_until, ruling_option, ruling_text, ruling_reason, answered_at, answered_by, \
         published_at, supersedes_id, superseded_by_id, withdrawn_reason, edited_at, edited_by, \
         created_at";

    pub(super) fn decision_from_row(r: &Row<'_>) -> rusqlite::Result<Decision> {
        let options: String = r.get(6)?;
        let kind: String = r.get(2)?;
        let priority: String = r.get(13)?;
        let state: String = r.get(16)?;
        let ruling_text: Option<String> = r.get(19)?;
        let answered_at: Option<String> = r.get(21)?;
        Ok(Decision {
            id: r.get(0)?,
            project_id: r.get(1)?,
            kind: DecisionKind::parse(&kind).unwrap_or(DecisionKind::Decision),
            title: r.get(3)?,
            normalised_title: r.get(4)?,
            body: r.get(5)?,
            options: serde_json::from_str(&options).unwrap_or_default(),
            recommendation: r.get(7)?,
            raised_by_bot_id: r.get(8)?,
            on_behalf_of_bot_id: r.get(9)?,
            origin_chain: r.get(10)?,
            source_message_id: r.get(11)?,
            source_task_id: r.get(12)?,
            priority: Priority::parse(&priority).unwrap_or(Priority::Normal),
            deadline_at: r.get::<_, Option<String>>(14)?.map(|s| parse_ts(&s)),
            deadline_notified_at: r.get::<_, Option<String>>(15)?.map(|s| parse_ts(&s)),
            state: DecisionState::parse(&state).unwrap_or(DecisionState::Open),
            held_until: r.get::<_, Option<String>>(17)?.map(|s| parse_ts(&s)),
            // A ruling exists once there are words for it; the option alone is
            // a half-written draft no bot should ever be shown.
            ruling: ruling_text.map(|text| Ruling {
                option: r.get(18).unwrap_or(None),
                text,
                reason: r.get(20).unwrap_or(None),
                answered_at: answered_at.as_deref().map(parse_ts).unwrap_or_else(now),
                answered_by: r
                    .get::<_, Option<String>>(22)
                    .unwrap_or(None)
                    .unwrap_or_default(),
            }),
            published_at: r.get::<_, Option<String>>(23)?.map(|s| parse_ts(&s)),
            supersedes_id: r.get(24)?,
            superseded_by_id: r.get(25)?,
            withdrawn_reason: r.get(26)?,
            edited_at: r.get::<_, Option<String>>(27)?.map(|s| parse_ts(&s)),
            edited_by: r.get(28)?,
            created_at: parse_ts(&r.get::<_, String>(29)?),
        })
    }

    pub fn get_decision(&self, id: &str) -> anyhow::Result<Option<Decision>> {
        let conn = self.lock();
        let sql = format!("SELECT {} FROM decision WHERE id = ?1", Self::DECISION_COLS);
        Ok(conn
            .query_row(&sql, params![id], Self::decision_from_row)
            .optional()?)
    }

    /// Insert a decision, refusing past the per-bot open cap.
    ///
    /// The cap is checked in the same transaction as the insert: a bot on a
    /// routine can raise faster than a sequential check would notice.
    pub fn insert_decision(&self, new: NewDecision<'_>) -> anyhow::Result<Decision> {
        let id = new_id();
        let created_at = now();
        let normalised = normalise_title(new.title);
        let options = serde_json::to_string(new.options)?;
        let settled = new.settled.is_some();
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        if !settled {
            let open: i64 = tx.query_row(
                "SELECT count(*) FROM decision
                  WHERE raised_by_bot_id = ?1 AND state IN ('open', 'answered', 'held')",
                params![new.raised_by_bot_id],
                |r| r.get(0),
            )?;
            if open >= MAX_OPEN_DECISIONS_PER_BOT {
                anyhow::bail!(
                    "refusing to raise: you already have {MAX_OPEN_DECISIONS_PER_BOT} decisions \
                     waiting on the owner — withdraw one with withdraw_decision, or wait for a \
                     ruling, before raising another"
                );
            }
        }
        let ruling = new.settled.as_ref();
        tx.execute(
            "INSERT INTO decision(id, project_id, kind, title, normalised_title, body,
                                  options_json, recommendation, raised_by_bot_id,
                                  on_behalf_of_bot_id, origin_chain, source_message_id,
                                  source_task_id, priority, deadline_at, state, ruling_option,
                                  ruling_text, ruling_reason, answered_at, answered_by,
                                  published_at, supersedes_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                     ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
            params![
                id,
                new.project_id,
                new.kind.as_str(),
                new.title,
                normalised,
                new.body,
                options,
                new.recommendation,
                new.raised_by_bot_id,
                new.on_behalf_of_bot_id,
                new.origin_chain,
                new.source_message_id,
                new.source_task_id,
                new.priority.as_str(),
                new.deadline_at.map(ts),
                if settled { "settled" } else { "open" },
                ruling.and_then(|r| r.option.clone()),
                ruling.map(|r| r.text.clone()),
                ruling.and_then(|r| r.reason.clone()),
                ruling.map(|r| ts(r.answered_at)),
                ruling.map(|r| r.answered_by.clone()),
                settled.then(|| ts(created_at)),
                new.supersedes_id,
                ts(created_at)
            ],
        )?;
        if let Some(old) = new.supersedes_id {
            tx.execute(
                "UPDATE decision SET superseded_by_id = ?2 WHERE id = ?1",
                params![old, id],
            )?;
        }
        tx.commit()?;
        drop(conn);
        self.get_decision(&id)?
            .ok_or_else(|| anyhow::anyhow!("decision vanished after insert"))
    }

    // ---- state machine ----
    //
    // Each gate names the state it expects and reports whether it was the one
    // that moved the row. Callers use that to make the side effect — a bus
    // delivery, a push — happen exactly once.

    /// Draft a ruling. Allowed from `open` (first answer) and `answered`
    /// (the owner changing their mind before publishing).
    pub fn try_answer(&self, id: &str, ruling: &Ruling) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE decision
                SET state = 'answered', ruling_option = ?2, ruling_text = ?3,
                    ruling_reason = ?4, answered_at = ?5, answered_by = ?6
              WHERE id = ?1 AND state IN ('open', 'answered')",
            params![
                id,
                ruling.option,
                ruling.text,
                ruling.reason,
                ts(ruling.answered_at),
                ruling.answered_by
            ],
        )?;
        Ok(changed == 1)
    }

    /// Throw the draft away and put it back on the pending list.
    pub fn try_unanswer(&self, id: &str) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE decision
                SET state = 'open', ruling_option = NULL, ruling_text = NULL,
                    ruling_reason = NULL, answered_at = NULL, answered_by = NULL
              WHERE id = ?1 AND state = 'answered'",
            params![id],
        )?;
        Ok(changed == 1)
    }

    /// Park it. `until` is a reminder, not a lock: the sweep resumes it, and
    /// the owner can resume it sooner.
    pub fn try_hold(
        &self,
        id: &str,
        until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE decision SET state = 'held', held_until = ?2
              WHERE id = ?1 AND state IN ('open', 'answered')",
            params![id, until.map(ts)],
        )?;
        Ok(changed == 1)
    }

    pub fn try_resume(&self, id: &str) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE decision SET state = 'open', held_until = NULL
              WHERE id = ?1 AND state = 'held'",
            params![id],
        )?;
        Ok(changed == 1)
    }

    /// Move a drafted ruling into the registry. Publishing is what makes it
    /// authority, so it is the one transition bots can observe.
    pub fn try_publish(&self, id: &str) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE decision SET state = 'settled', published_at = ?2
              WHERE id = ?1 AND state = 'answered' AND ruling_text IS NOT NULL",
            params![id, ts(now())],
        )?;
        Ok(changed == 1)
    }

    pub fn try_withdraw(&self, id: &str, reason: &str) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE decision SET state = 'withdrawn', withdrawn_reason = ?2
              WHERE id = ?1 AND state IN ('open', 'answered', 'held')",
            params![id, reason],
        )?;
        Ok(changed == 1)
    }

    /// Promote a bot-relayed ruling to one the owner typed.
    ///
    /// `record_decision` keeps the provenance honest by storing
    /// `owner-via-bot:<id>`, and bots are told to read that as a relay with a
    /// paper trail. Confirming is the owner saying yes, that is what I said.
    pub fn try_confirm(&self, id: &str) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE decision SET answered_by = 'owner'
              WHERE id = ?1 AND state = 'settled' AND answered_by LIKE 'owner-via-bot:%'",
            params![id],
        )?;
        Ok(changed == 1)
    }

    /// Relayed rulings this bot has filed that the owner has not confirmed.
    ///
    /// `record_decision` files a settled row, so the open cap in
    /// [`Db::insert_decision`] does not apply to it. This is the ceiling that
    /// does: confirming is the owner saying "yes, that is what I said", and a
    /// bot should not be able to accumulate unbounded unverified authority.
    pub fn count_unconfirmed_relays(&self, bot_id: &str) -> anyhow::Result<i64> {
        Ok(self.lock().query_row(
            "SELECT count(*) FROM decision
              WHERE raised_by_bot_id = ?1 AND state = 'settled'
                AND answered_by LIKE 'owner-via-bot:%'",
            params![bot_id],
            |r| r.get(0),
        )?)
    }

    /// Stamp the one-time deadline warning, reporting whether this call is the
    /// one that should send it.
    pub fn try_mark_deadline_notified(&self, id: &str) -> anyhow::Result<bool> {
        let changed = self.lock().execute(
            "UPDATE decision SET deadline_notified_at = ?2
              WHERE id = ?1 AND deadline_notified_at IS NULL",
            params![id, ts(now())],
        )?;
        Ok(changed == 1)
    }
}
