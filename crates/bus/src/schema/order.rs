//! Migration 15: a user-chosen order for the bots in each project.
//!
//! Until now the roster was `ORDER BY name`, so a rename was the only way to
//! move a bot. `sort_order` is a dense per-project position; it is backfilled
//! from that name order so an upgraded sidebar looks exactly as it did before.
//! Archived rows are numbered too, which is harmless: only live bots are
//! listed, and their relative order is what the backfill preserves.

pub(super) const MIGRATION_15: &str = r#"
ALTER TABLE bot ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;

UPDATE bot SET sort_order = (
    SELECT count(*) FROM bot AS earlier
     WHERE earlier.project_id = bot.project_id
       AND earlier.name < bot.name
);

CREATE INDEX idx_bot_order ON bot(project_id, sort_order);
"#;
