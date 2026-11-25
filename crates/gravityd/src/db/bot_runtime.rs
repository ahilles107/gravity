use bus::{new_id, now, Bot, BotRuntime, BotState};
use rusqlite::params;

use super::{ts, Db};

impl Db {
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
        self.create_bot_with_runtime(
            project_id,
            name,
            description,
            instructions,
            avatar,
            workspace_path,
            dir_name,
            created_by_bot_id,
            BotRuntime::ClaudeCode,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_bot_with_runtime(
        &self,
        project_id: &str,
        name: &str,
        description: &str,
        instructions: &str,
        avatar: &str,
        workspace_path: &str,
        dir_name: &str,
        created_by_bot_id: Option<&str>,
        runtime: BotRuntime,
    ) -> anyhow::Result<Bot> {
        let mut bot = Bot {
            id: new_id(),
            project_id: project_id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            avatar: avatar.to_string(),
            instructions: instructions.to_string(),
            runtime,
            state: BotState::Stopped,
            state_reason: String::new(),
            unread_count: 0,
            workspace_path: workspace_path.to_string(),
            dir_name: dir_name.to_string(),
            created_by_bot_id: created_by_bot_id.map(|s| s.to_string()),
            deleted_at: None,
            peer_id: None,
            remote_bot_id: None,
            sort_order: 0,
            created_at: now(),
        };
        let conn = self.lock();
        // A new bot joins the end of its project's roster, wherever its name
        // would sort, so creating one never reshuffles what the owner arranged.
        bot.sort_order = conn.query_row(
            "SELECT coalesce(max(sort_order) + 1, 0) FROM bot
              WHERE project_id = ?1 AND deleted_at IS NULL",
            params![bot.project_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO bot(id, project_id, name, description, avatar, instructions,
                             workspace_path, created_at, dir_name, created_by_bot_id, runtime,
                             sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                bot.created_by_bot_id,
                bot.runtime.as_str(),
                bot.sort_order
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

    pub fn set_bot_runtime(&self, bot_id: &str, runtime: BotRuntime) -> anyhow::Result<()> {
        let changed = self.lock().execute(
            "UPDATE bot SET runtime = ?2 WHERE id = ?1 AND deleted_at IS NULL",
            params![bot_id, runtime.as_str()],
        )?;
        anyhow::ensure!(changed == 1, "bot not found or archived");
        Ok(())
    }
}
