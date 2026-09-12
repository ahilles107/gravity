//! The tag taxonomy, shared by every project.
//!
//! One taxonomy rather than one per project: a bot filing `spend` in Self
//! Hosted means what a bot filing `spend` in Quok Labs does, and the registry
//! is worth more when a ruling can be found across both. Names are lowercased
//! on write, so the plain `UNIQUE` on `tag.name` is a case-insensitive one.

use bus::*;
use rusqlite::{params, OptionalExtension, Row};

use super::{parse_ts, ts, Db};

/// A tag's id, or a worded refusal. Bare `query_row` here surfaced
/// `QueryReturnedNoRows` to a bot that simply named a tag that does not exist.
fn tag_id(tx: &rusqlite::Transaction<'_>, name: &str) -> anyhow::Result<String> {
    tx.query_row("SELECT id FROM tag WHERE name = ?1", params![name], |r| {
        r.get(0)
    })
    .optional()?
    .ok_or_else(|| {
        crate::decisions::not_found(format!(
            "no tag named '{name}' — list_tags shows what exists"
        ))
    })
}

impl Db {
    const TAG_COLS: &'static str =
        "id, name, description, color, created_by, retired_at, created_at";

    fn tag_from_row(r: &Row<'_>) -> rusqlite::Result<Tag> {
        Ok(Tag {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            color: r.get(3)?,
            created_by: r.get(4)?,
            retired_at: r.get::<_, Option<String>>(5)?.map(|s| parse_ts(&s)),
            created_at: parse_ts(&r.get::<_, String>(6)?),
        })
    }

    pub fn get_tag(&self, name: &str) -> anyhow::Result<Option<Tag>> {
        let conn = self.lock();
        let sql = format!("SELECT {} FROM tag WHERE name = ?1", Self::TAG_COLS);
        Ok(conn
            .query_row(&sql, params![name.to_lowercase()], Self::tag_from_row)
            .optional()?)
    }

    /// Create a tag, or update the description and colour of an existing one.
    /// Upsert rather than create so a bot describing a tag someone else made
    /// improves the taxonomy instead of failing.
    pub fn upsert_tag(
        &self,
        name: &str,
        description: Option<&str>,
        color: Option<&str>,
        created_by: &str,
    ) -> anyhow::Result<Tag> {
        let name = name.to_lowercase();
        self.lock().execute(
            "INSERT INTO tag(id, name, description, color, created_by, created_at)
             VALUES (?1, ?2, COALESCE(?3, ''), COALESCE(?4, ''), ?5, ?6)
             ON CONFLICT(name) DO UPDATE SET
                 description = COALESCE(?3, description),
                 color = COALESCE(?4, color),
                 retired_at = NULL",
            params![new_id(), name, description, color, created_by, ts(now())],
        )?;
        self.get_tag(&name)?
            .ok_or_else(|| anyhow::anyhow!("tag vanished after upsert"))
    }

    /// Every tag with how many decisions use it, per project.
    pub fn list_tags_with_uses(&self) -> anyhow::Result<Vec<TagUsage>> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare(&format!("SELECT {} FROM tag ORDER BY name", Self::TAG_COLS))?;
        let tags: Vec<Tag> = stmt
            .query_map([], Self::tag_from_row)?
            .collect::<Result<_, _>>()?;
        let mut uses: std::collections::HashMap<String, std::collections::BTreeMap<String, i64>> =
            std::collections::HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT dt.tag_id, d.project_id, count(*) FROM decision_tag dt
               JOIN decision d ON d.id = dt.decision_id
              GROUP BY dt.tag_id, d.project_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (tag_id, project_id, count) = row?;
            uses.entry(tag_id).or_default().insert(project_id, count);
        }
        // Recency and what is outstanding are global rather than per-project:
        // they say whether the tag is alive, which is a taxonomy question.
        let mut recency: std::collections::HashMap<String, (Option<String>, i64)> =
            std::collections::HashMap::new();
        let mut stmt = conn.prepare(
            "SELECT dt.tag_id, max(COALESCE(d.published_at, d.created_at)),
                    sum(d.state IN ('open', 'answered', 'held'))
               FROM decision_tag dt
               JOIN decision d ON d.id = dt.decision_id
              GROUP BY dt.tag_id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (tag_id, last_used_at, open_uses) = row?;
            recency.insert(tag_id, (last_used_at, open_uses));
        }
        Ok(tags
            .into_iter()
            .map(|tag| {
                let uses = uses.remove(&tag.id).unwrap_or_default();
                let (last_used_at, open_uses) = recency.remove(&tag.id).unwrap_or_default();
                TagUsage {
                    tag,
                    uses,
                    last_used_at: last_used_at.as_deref().map(parse_ts),
                    open_uses,
                }
            })
            .collect())
    }

    /// How many settled decisions carry a tag. Bots may not retire a tag past
    /// a threshold of these, because unfiling settled history is the owner's
    /// call, not a tidy-up.
    pub fn settled_uses(&self, tag_id: &str) -> anyhow::Result<i64> {
        Ok(self.lock().query_row(
            "SELECT count(*) FROM decision_tag dt JOIN decision d ON d.id = dt.decision_id
              WHERE dt.tag_id = ?1 AND d.state = 'settled'",
            params![tag_id],
            |r| r.get(0),
        )?)
    }

    /// Projects other than this one that file decisions under a tag.
    ///
    /// The taxonomy is shared, so retiring or merging a tag reaches every
    /// project at once. A bot tidying its own project needs to know when that
    /// would reach somebody else's.
    pub fn tag_other_projects(
        &self,
        tag_id: &str,
        project_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT d.project_id FROM decision_tag dt
               JOIN decision d ON d.id = dt.decision_id
              WHERE dt.tag_id = ?1 AND d.project_id != ?2
              ORDER BY d.project_id",
        )?;
        let rows = stmt.query_map(params![tag_id, project_id], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Retire a tag, optionally relinking everything it held onto another.
    ///
    /// Retiring hides it from pickers but keeps its links, so a settled
    /// decision never loses the category it was filed under.
    pub fn retire_tag(&self, name: &str, into: Option<&str>) -> anyhow::Result<Tag> {
        let name = name.to_lowercase();
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let from_id: String = tag_id(&tx, &name)?;
        if let Some(into) = into {
            let into_id: String = tag_id(&tx, &into.to_lowercase())?;
            if into_id == from_id {
                anyhow::bail!("cannot merge a tag into itself");
            }
            // `OR IGNORE` because a decision may already carry both tags, and
            // the merge should leave it with one rather than fail.
            tx.execute(
                "INSERT OR IGNORE INTO decision_tag(decision_id, tag_id)
                 SELECT decision_id, ?2 FROM decision_tag WHERE tag_id = ?1",
                params![from_id, into_id],
            )?;
            tx.execute(
                "DELETE FROM decision_tag WHERE tag_id = ?1",
                params![from_id],
            )?;
        }
        tx.execute(
            "UPDATE tag SET retired_at = ?2 WHERE id = ?1",
            params![from_id, ts(now())],
        )?;
        tx.commit()?;
        drop(conn);
        self.get_tag(&name)?
            .ok_or_else(|| anyhow::anyhow!("tag vanished after retire"))
    }

    /// Rename a tag in place. Decisions reference it by id, so every filing
    /// follows the new name rather than being relinked.
    pub fn rename_tag(&self, name: &str, to: &str) -> anyhow::Result<Tag> {
        let (name, to) = (name.to_lowercase(), to.to_lowercase());
        let changed = self.lock().execute(
            "UPDATE tag SET name = ?2 WHERE name = ?1",
            params![name, to],
        )?;
        if changed == 0 {
            anyhow::bail!("no tag named '{name}'");
        }
        self.get_tag(&to)?
            .ok_or_else(|| anyhow::anyhow!("tag vanished after rename"))
    }

    /// Delete a tag and unfile everything it held. The decisions stay; only
    /// the category goes, which is why this is the owner's alone.
    pub fn delete_tag(&self, name: &str) -> anyhow::Result<bool> {
        let name = name.to_lowercase();
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let id: Option<String> = tx
            .query_row("SELECT id FROM tag WHERE name = ?1", params![name], |r| {
                r.get(0)
            })
            .optional()?;
        let Some(id) = id else {
            return Ok(false);
        };
        tx.execute("DELETE FROM decision_tag WHERE tag_id = ?1", params![id])?;
        tx.execute("DELETE FROM tag WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(true)
    }

    /// Replace a decision's tags with exactly this set, creating any that are
    /// new. The caller has already validated the names.
    pub fn set_decision_tags(
        &self,
        decision_id: &str,
        names: &[String],
        created_by: &str,
    ) -> anyhow::Result<Vec<String>> {
        let names: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM decision_tag WHERE decision_id = ?1",
            params![decision_id],
        )?;
        for name in &names {
            tx.execute(
                "INSERT INTO tag(id, name, created_by, created_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(name) DO NOTHING",
                params![new_id(), name, created_by, ts(now())],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO decision_tag(decision_id, tag_id)
                 SELECT ?1, id FROM tag WHERE name = ?2",
                params![decision_id, name],
            )?;
        }
        tx.commit()?;
        Ok(names)
    }

    /// The tags on each of these decisions, in one query.
    pub fn tags_for(
        &self,
        decision_ids: &[String],
    ) -> anyhow::Result<std::collections::HashMap<String, Vec<String>>> {
        let mut out: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        if decision_ids.is_empty() {
            return Ok(out);
        }
        let placeholders = vec!["?"; decision_ids.len()].join(", ");
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT dt.decision_id, t.name FROM decision_tag dt JOIN tag t ON t.id = dt.tag_id
              WHERE dt.decision_id IN ({placeholders}) ORDER BY t.name"
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(decision_ids), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, name) = row?;
            out.entry(id).or_default().push(name);
        }
        Ok(out)
    }
}
