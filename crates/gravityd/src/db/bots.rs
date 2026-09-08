//! Bot rows: creation, identity edits, and archival deletion.
//!
//! Every query that resolves a bot for *addressing* or *counting* filters on
//! `deleted_at IS NULL`. That predicate lives in the shared constants below
//! rather than at each call site — getting it wrong in one place would
//! resurrect an archived bot as a message target.

use bus::*;
use rusqlite::{params, OptionalExtension, Row};

use super::{parse_ts, ts, Db};

/// Marker separating an archived bot's original name from its id suffix. Not
/// in the validated name charset, so a tombstone can never collide with a live
/// name. See `MIGRATION_3`.
const TOMBSTONE_SEP: char = '#';

impl Db {
    // ---- bots ----

    fn bot_from_row(r: &Row<'_>) -> rusqlite::Result<Bot> {
        Ok(Bot {
            id: r.get(0)?,
            project_id: r.get(1)?,
            name: r.get(2)?,
            description: r.get(3)?,
            avatar: r.get(4)?,
            instructions: r.get(5)?,
            workspace_path: r.get(6)?,
            created_at: parse_ts(&r.get::<_, String>(7)?),
            dir_name: r.get(8)?,
            created_by_bot_id: r.get(9)?,
            deleted_at: r.get::<_, Option<String>>(10)?.map(|s| parse_ts(&s)),
            // Runtime fields are overlaid by the supervisor.
            state: BotState::Stopped,
            state_reason: String::new(),
            unread_count: 0,
        })
    }

    const BOT_COLS: &'static str = "id, project_id, name, description, avatar, instructions, \
         workspace_path, created_at, dir_name, created_by_bot_id, deleted_at";

    /// Restricts a query to bots that still exist for addressing purposes.
    const LIVE: &'static str = "deleted_at IS NULL";

    #[allow(clippy::too_many_arguments)]
    pub fn create_bot(
        &self,
        project_id: &str,
        name: &str,
        description: &str,
        instructions: &str,
        avatar: &str,
        workspace_path: &str,
        dir_name: &str,
        created_by_bot_id: Option<&str>,
    ) -> anyhow::Result<Bot> {
        let bot = Bot {
            id: new_id(),
            project_id: project_id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            avatar: avatar.to_string(),
            instructions: instructions.to_string(),
            state: BotState::Stopped,
            state_reason: String::new(),
            unread_count: 0,
            workspace_path: workspace_path.to_string(),
            dir_name: dir_name.to_string(),
            created_by_bot_id: created_by_bot_id.map(|s| s.to_string()),
            deleted_at: None,
            created_at: now(),
        };
        let conn = self.lock();
        conn.execute(
            "INSERT INTO bot(id, project_id, name, description, avatar, instructions,
                             workspace_path, created_at, dir_name, created_by_bot_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                bot.id,
                bot.project_id,
                bot.name,
                bot.description,
                bot.avatar,
                bot.instructions,
                bot.workspace_path,
                ts(bot.created_at),
                bot.dir_name,
                bot.created_by_bot_id
            ],
        )?;
        // A DM conversation exists for every bot from creation.
        let conv_id = new_id();
        conn.execute(
            "INSERT INTO conversation(id, project_id, kind, bot_id, title, created_at)
             VALUES (?1, ?2, 'dm', ?3, ?4, ?5)",
            params![conv_id, bot.project_id, bot.id, bot.name, ts(now())],
        )?;
        Ok(bot)
    }

    /// Record that the bot's workspace now holds a Claude Code conversation,
    /// so later starts resume it instead of opening a blank session.
    pub fn mark_bot_session(&self, bot_id: &str) -> anyhow::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE bot SET has_session = 1 WHERE id = ?1 AND has_session = 0",
            params![bot_id],
        )?;
        Ok(())
    }

    /// True once the bot has started a session at least once. Survives daemon
    /// restarts, which is the whole point: the conversation outlives the
    /// process that hosted it.
    pub fn bot_has_session(&self, bot_id: &str) -> anyhow::Result<bool> {
        let conn = self.lock();
        let has: Option<i64> = conn
            .query_row(
                "SELECT has_session FROM bot WHERE id = ?1",
                params![bot_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(has.unwrap_or(0) != 0)
    }

    /// The model this bot's sessions are pinned to, if one has been observed.
    /// `None` leaves the choice to Claude Code's own settings.
    pub fn bot_model(&self, bot_id: &str) -> anyhow::Result<Option<String>> {
        let conn = self.lock();
        let model: Option<Option<String>> = conn
            .query_row(
                "SELECT model FROM bot WHERE id = ?1",
                params![bot_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(model.flatten().filter(|m| !m.is_empty()))
    }

    /// Pin the model the bot's session is running, or clear the pin with
    /// `None` so the next start falls back to Claude Code's own default.
    pub fn set_bot_model(&self, bot_id: &str, model: Option<&str>) -> anyhow::Result<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE bot SET model = ?2 WHERE id = ?1",
            params![bot_id, model],
        )?;
        Ok(())
    }

    /// Apply identity edits. Only fields present in the argument list change;
    /// `None` leaves the stored value alone.
    pub fn update_bot(
        &self,
        bot_id: &str,
        name: Option<&str>,
        description: Option<&str>,
        instructions: Option<&str>,
        avatar: Option<&str>,
    ) -> anyhow::Result<()> {
        let conn = self.lock();
        for (column, value) in [
            ("name", name),
            ("description", description),
            ("instructions", instructions),
            ("avatar", avatar),
        ] {
            let Some(value) = value else {
                continue;
            };
            conn.execute(
                &format!("UPDATE bot SET {column} = ?2 WHERE id = ?1"),
                params![bot_id, value],
            )?;
        }
        if let Some(name) = name {
            // The DM conversation is titled after the bot; keep them in step.
            conn.execute(
                "UPDATE conversation SET title = ?2 WHERE bot_id = ?1",
                params![bot_id, name],
            )?;
        }
        Ok(())
    }

    /// Archive a bot: it leaves `list_bots`, addressing and the population cap,
    /// but its row and history survive so past messages stay attributable.
    ///
    /// The name is tombstoned so it can be reused immediately — create, delete,
    /// create is an expected pattern now that bots manage each other.
    /// Callers are responsible for stopping the runtime, revoking the token and
    /// cancelling open tasks first; see `botmgmt::archive_bot`.
    pub fn archive_bot(&self, bot_id: &str, deleted_by: &str) -> anyhow::Result<()> {
        let conn = self.lock();
        let name: String =
            conn.query_row("SELECT name FROM bot WHERE id = ?1", params![bot_id], |r| {
                r.get(0)
            })?;
        let suffix: String = bot_id.chars().take(8).collect();
        let tombstone = format!("{name}{TOMBSTONE_SEP}{suffix}");
        conn.execute(
            "UPDATE bot SET deleted_at = ?2, deleted_by = ?3, name = ?4 WHERE id = ?1",
            params![bot_id, ts(now()), deleted_by, tombstone],
        )?;
        conn.execute(
            "UPDATE conversation SET archived_at = ?2 WHERE bot_id = ?1",
            params![bot_id, ts(now())],
        )?;
        // Routines would otherwise keep firing prompts at a dead runtime.
        conn.execute(
            "DELETE FROM routine_run WHERE routine_id IN (SELECT id FROM routine WHERE bot_id = ?1)",
            params![bot_id],
        )?;
        conn.execute("DELETE FROM routine WHERE bot_id = ?1", params![bot_id])?;
        // Undelivered mail has nowhere to land.
        conn.execute(
            "DELETE FROM inbox WHERE delivery_id IN
               (SELECT id FROM delivery WHERE bot_id = ?1 AND state IN ('queued', 'leased'))",
            params![bot_id],
        )?;
        conn.execute(
            "DELETE FROM delivery WHERE bot_id = ?1 AND state IN ('queued', 'leased')",
            params![bot_id],
        )?;
        Ok(())
    }

    /// The original name of an archived bot, or its current name if live.
    pub fn display_name(bot: &Bot) -> String {
        match bot.deleted_at {
            Some(_) => bot
                .name
                .rsplit_once(TOMBSTONE_SEP)
                .map(|(base, _)| base.to_string())
                .unwrap_or_else(|| bot.name.clone()),
            None => bot.name.clone(),
        }
    }

    pub fn list_bots(&self, project_id: Option<&str>) -> anyhow::Result<Vec<Bot>> {
        self.list_bots_inner(project_id, false)
    }

    /// Includes archived bots. Used by history views and retention, never by
    /// addressing.
    pub fn list_bots_with_archived(&self, project_id: Option<&str>) -> anyhow::Result<Vec<Bot>> {
        self.list_bots_inner(project_id, true)
    }

    fn list_bots_inner(
        &self,
        project_id: Option<&str>,
        include_archived: bool,
    ) -> anyhow::Result<Vec<Bot>> {
        let conn = self.lock();
        let mut clauses: Vec<&str> = Vec::new();
        if project_id.is_some() {
            clauses.push("project_id = ?1");
        }
        if !include_archived {
            clauses.push(Self::LIVE);
        }
        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        let sql = format!(
            "SELECT {} FROM bot {where_sql} ORDER BY name",
            Self::BOT_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = match project_id {
            Some(pid) => stmt.query_map(params![pid], Self::bot_from_row)?,
            None => stmt.query_map([], Self::bot_from_row)?,
        };
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Fetch by id regardless of archival state — history and revert need to
    /// resolve bots that are gone. Callers doing addressing must check
    /// `deleted_at`, or use [`Db::get_live_bot`].
    pub fn get_bot(&self, bot_id: &str) -> anyhow::Result<Option<Bot>> {
        let conn = self.lock();
        let sql = format!("SELECT {} FROM bot WHERE id = ?1", Self::BOT_COLS);
        Ok(conn
            .query_row(&sql, params![bot_id], Self::bot_from_row)
            .optional()?)
    }

    pub fn get_live_bot(&self, bot_id: &str) -> anyhow::Result<Option<Bot>> {
        Ok(self.get_bot(bot_id)?.filter(|b| b.deleted_at.is_none()))
    }

    pub fn get_bot_by_name(&self, project_id: &str, name: &str) -> anyhow::Result<Option<Bot>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {} FROM bot WHERE project_id = ?1 AND name = ?2 COLLATE NOCASE AND {}",
            Self::BOT_COLS,
            Self::LIVE
        );
        Ok(conn
            .query_row(&sql, params![project_id, name], Self::bot_from_row)
            .optional()?)
    }

    /// Live bots in a project — the number the population cap applies to.
    pub fn count_live_bots(&self, project_id: &str) -> anyhow::Result<i64> {
        let conn = self.lock();
        Ok(conn.query_row(
            &format!(
                "SELECT count(*) FROM bot WHERE project_id = ?1 AND {}",
                Self::LIVE
            ),
            params![project_id],
            |r| r.get(0),
        )?)
    }

    /// Whether this project has ever had a bot, deleted ones included.
    pub fn project_has_had_bots(&self, project_id: &str) -> anyhow::Result<bool> {
        let conn = self.lock();
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM bot WHERE project_id = ?1",
            params![project_id],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    /// Whether a directory name is already in use in this project.
    ///
    /// Archived bots still count: their workspace stays on disk until retention
    /// reclaims it, so reusing the directory would hand a new bot another bot's
    /// files. This is the check that catches `"Bot A"` vs `"Bot-A"`, which are
    /// distinct names that sanitize to one directory.
    pub fn dir_name_taken(&self, project_id: &str, dir_name: &str) -> anyhow::Result<bool> {
        let conn = self.lock();
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM bot WHERE project_id = ?1 AND dir_name = ?2 COLLATE NOCASE",
            params![project_id, dir_name],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    /// Bots created by the given bot and still live.
    pub fn children_of(&self, bot_id: &str) -> anyhow::Result<Vec<Bot>> {
        let conn = self.lock();
        let sql = format!(
            "SELECT {} FROM bot WHERE created_by_bot_id = ?1 AND {} ORDER BY name",
            Self::BOT_COLS,
            Self::LIVE
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![bot_id], Self::bot_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
