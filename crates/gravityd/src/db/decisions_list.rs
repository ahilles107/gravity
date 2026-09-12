//! Reading the registry: filtered lists, the badge counts, full-text search,
//! and the near-duplicate lookup that keeps a settled topic from coming back
//! as a fresh finding.

use bus::*;
use rusqlite::{params_from_iter, types::Value as SqlValue, OptionalExtension};

use super::{ts, Db};

/// What a caller wants out of the registry. Every field narrows; `None` means
/// "do not narrow on this".
#[derive(Default)]
pub struct DecisionFilter<'a> {
    pub project_id: Option<&'a str>,
    /// Empty means every state except `withdrawn`, which is history.
    pub states: &'a [DecisionState],
    pub tag: Option<&'a str>,
    pub bot_id: Option<&'a str>,
    /// Full-text query over title, body and ruling.
    pub query: Option<&'a str>,
    /// Cursor: return records ordered after this one. Composite because
    /// neither a deadline nor a timestamp is unique.
    pub before: Option<&'a str>,
    pub limit: i64,
}

/// Pending decisions are sorted by what runs out first, then by what has
/// waited longest. A decision with no deadline sorts after every dated one.
const PENDING_ORDER: &str = "COALESCE(d.deadline_at, '9999') ASC, d.created_at ASC, d.id ASC";

impl Db {
    /// List decisions matching a filter, newest deadline first.
    pub fn list_decisions(&self, filter: &DecisionFilter<'_>) -> anyhow::Result<Vec<Decision>> {
        let mut wheres: Vec<String> = Vec::new();
        let mut args: Vec<SqlValue> = Vec::new();
        let bind = |value: SqlValue, args: &mut Vec<SqlValue>| -> String {
            args.push(value);
            format!("?{}", args.len())
        };
        if let Some(project_id) = filter.project_id {
            let p = bind(project_id.to_string().into(), &mut args);
            wheres.push(format!("d.project_id = {p}"));
        }
        if filter.states.is_empty() {
            wheres.push("d.state != 'withdrawn'".to_string());
        } else {
            let placeholders: Vec<String> = filter
                .states
                .iter()
                .map(|s| bind(s.as_str().to_string().into(), &mut args))
                .collect();
            wheres.push(format!("d.state IN ({})", placeholders.join(", ")));
        }
        if let Some(bot_id) = filter.bot_id {
            let b = bind(bot_id.to_string().into(), &mut args);
            wheres.push(format!(
                "(d.raised_by_bot_id = {b} OR d.on_behalf_of_bot_id = {b})"
            ));
        }
        if let Some(tag) = filter.tag {
            let t = bind(tag.to_lowercase().into(), &mut args);
            wheres.push(format!(
                "d.id IN (SELECT dt.decision_id FROM decision_tag dt
                            JOIN tag g ON g.id = dt.tag_id WHERE g.name = {t})"
            ));
        }
        // Quoted, never raw: FTS5 parses the *bound value* as query syntax, so
        // a caller's plain question is a parse error rather than a search.
        if let Some(query) = filter.query.map(fts_query).filter(|q| !q.is_empty()) {
            let q = bind(query.into(), &mut args);
            wheres.push(format!(
                "d.rowid IN (SELECT rowid FROM decision_fts WHERE decision_fts MATCH {q})"
            ));
        }
        if let Some(before) = filter.before {
            let c = bind(before.to_string().into(), &mut args);
            // Same key as the sort, so a cursor never skips or repeats a row
            // two decisions share a timestamp with.
            wheres.push(format!(
                "(COALESCE(d.deadline_at, '9999'), d.created_at, d.id) >
                 (SELECT COALESCE(c.deadline_at, '9999'), c.created_at, c.id
                    FROM decision c WHERE c.id = {c})"
            ));
        }
        let limit = bind(filter.limit.clamp(1, 500).into(), &mut args);
        let sql = format!(
            "SELECT {} FROM decision d WHERE {} ORDER BY {PENDING_ORDER} LIMIT {limit}",
            Self::DECISION_COLS
                .split(", ")
                .map(|c| format!("d.{c}"))
                .collect::<Vec<_>>()
                .join(", "),
            wheres.join(" AND ")
        );
        let conn = self.lock();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args), Self::decision_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The badge: what the owner still owes an answer on.
    ///
    /// A drafted-but-unpublished ruling counts — it is work the owner started
    /// and has not finished. A held decision does not: parking it was itself a
    /// decision, and counting it would make "later" feel like a backlog.
    pub fn count_pending_decisions(&self, due_within_hours: i64) -> anyhow::Result<PendingCounts> {
        let soon = ts(now() + chrono::Duration::hours(due_within_hours));
        let conn = self.lock();
        let mut counts = PendingCounts::default();
        let mut stmt = conn.prepare(
            "SELECT project_id,
                    count(*),
                    sum(CASE WHEN priority = 'urgent' THEN 1 ELSE 0 END),
                    sum(CASE WHEN deadline_at IS NOT NULL AND deadline_at <= ?1 THEN 1 ELSE 0 END)
               FROM decision
              WHERE state IN ('open', 'answered')
              GROUP BY project_id",
        )?;
        let rows = stmt.query_map(params_from_iter([SqlValue::from(soon)]), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                r.get::<_, Option<i64>>(3)?.unwrap_or(0),
            ))
        })?;
        for row in rows {
            let (project_id, total, urgent, due_soon) = row?;
            counts.by_project.insert(project_id, total);
            counts.total += total;
            counts.urgent += urgent;
            counts.due_soon += due_soon;
        }
        Ok(counts)
    }

    /// Decisions this bot raised or is waiting on, for `check_inbox`.
    pub fn open_decisions_for(&self, bot_id: &str) -> anyhow::Result<Vec<Decision>> {
        self.list_decisions(&DecisionFilter {
            bot_id: Some(bot_id),
            states: &[
                DecisionState::Open,
                DecisionState::Answered,
                DecisionState::Held,
            ],
            limit: MAX_OPEN_DECISIONS_PER_BOT,
            ..Default::default()
        })
    }

    /// Decisions whose deadline falls inside the window and that have not yet
    /// raised their one-time warning.
    pub fn decisions_due_within(&self, hours: i64) -> anyhow::Result<Vec<Decision>> {
        let cutoff = ts(now() + chrono::Duration::hours(hours));
        let conn = self.lock();
        let sql = format!(
            "SELECT {} FROM decision
              WHERE state IN ('open', 'answered')
                AND deadline_at IS NOT NULL AND deadline_at <= ?1
                AND deadline_notified_at IS NULL",
            Self::DECISION_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params_from_iter([SqlValue::from(cutoff)]),
            Self::decision_from_row,
        )?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Held decisions whose `held_until` has passed.
    pub fn held_expired(&self) -> anyhow::Result<Vec<Decision>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {} FROM decision
              WHERE state = 'held' AND held_until IS NOT NULL AND held_until <= ?1",
            Self::DECISION_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params_from_iter([SqlValue::from(ts(now()))]),
            Self::decision_from_row,
        )?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// An open decision the same bot already raised with the same question.
    ///
    /// Exact rather than fuzzy on purpose: this one refuses the raise, so it
    /// has to be something the bot can see it did.
    pub fn open_duplicate(
        &self,
        project_id: &str,
        bot_id: &str,
        title: &str,
    ) -> anyhow::Result<Option<Decision>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {} FROM decision
              WHERE project_id = ?1 AND raised_by_bot_id = ?2 AND normalised_title = ?3
                AND state IN ('open', 'answered', 'held')
              LIMIT 1",
            Self::DECISION_COLS
        );
        let normalised = super::decisions::normalise_title(title);
        Ok(conn
            .query_row(
                &sql,
                params_from_iter([
                    SqlValue::from(project_id.to_string()),
                    SqlValue::from(bot_id.to_string()),
                    SqlValue::from(normalised),
                ]),
                Self::decision_from_row,
            )
            .optional()?)
    }

    /// Decisions that look like the one about to be raised — same project, and
    /// either a shared tag or a full-text hit on the title.
    ///
    /// This is advisory, not a refusal: it is handed back so the bot reads a
    /// settled ruling before re-opening a closed topic.
    pub fn near_duplicates(
        &self,
        project_id: &str,
        title: &str,
        tags: &[String],
        limit: i64,
    ) -> anyhow::Result<Vec<Decision>> {
        let mut hits = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let terms = fts_terms(title);
        if !terms.is_empty() {
            for d in self.list_decisions(&DecisionFilter {
                project_id: Some(project_id),
                query: Some(&terms),
                limit,
                ..Default::default()
            })? {
                if seen.insert(d.id.clone()) {
                    hits.push(d);
                }
            }
        }
        for tag in tags {
            if hits.len() as i64 >= limit {
                break;
            }
            for d in self.list_decisions(&DecisionFilter {
                project_id: Some(project_id),
                tag: Some(tag),
                states: &[DecisionState::Settled],
                limit,
                ..Default::default()
            })? {
                if seen.insert(d.id.clone()) {
                    hits.push(d);
                }
            }
        }
        hits.truncate(limit.max(0) as usize);
        Ok(hits)
    }
}

/// Split text into the words FTS5 will index, discarding punctuation.
fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
}

/// Turn a title into an FTS5 OR-query of its longer words.
///
/// Quoting each term is what keeps a title containing `AND`, `*` or a quote
/// from being read as query syntax and raising instead of matching.
fn fts_terms(title: &str) -> String {
    words(&super::decisions::normalise_title(title))
        .filter(|w| w.len() > 3)
        .take(8)
        .map(|w| format!("\"{w}\""))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// Turn caller-supplied text into an FTS5 query of quoted terms, ANDed.
///
/// A bot told the field is "full-text over title, body and ruling" writes a
/// sentence, and `cost:high`, `a AND`, or an unclosed quote is an FTS5 parse
/// error surfaced as `internal` rather than a search. Quoting each word is the
/// same treatment [`fts_terms`] already gives the near-duplicate lookup.
/// Empty — a query of pure punctuation — means do not narrow at all, matching
/// how a blank query behaves.
fn fts_query(text: &str) -> String {
    words(text)
        .take(16)
        .map(|w| format!("\"{w}\""))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::fts_query;

    #[test]
    fn quotes_every_term_and_drops_syntax() {
        assert_eq!(fts_query("postgres"), "\"postgres\"");
        assert_eq!(
            fts_query("should we use \"postgres"),
            "\"should\" \"we\" \"use\" \"postgres\""
        );
        assert_eq!(fts_query("cost:high"), "\"cost\" \"high\"");
        assert_eq!(fts_query("a AND"), "\"a\" \"AND\"");
        assert_eq!(fts_query("what about (this"), "\"what\" \"about\" \"this\"");
    }

    #[test]
    fn punctuation_only_does_not_narrow() {
        assert_eq!(fts_query("  ?? -- "), "");
    }
}
