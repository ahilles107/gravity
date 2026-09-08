//! Key-value settings in the `meta` table, alongside `schema_version`.
//! Backing store for config values the control plane may change at runtime.

use rusqlite::OptionalExtension;

use super::Db;

impl Db {
    pub fn get_meta(&self, key: &str) -> anyhow::Result<Option<String>> {
        let conn = self.lock();
        let value = conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(value)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::db::Db;

    #[test]
    fn meta_round_trips_and_overwrites() {
        let db = Db::open_in_memory().expect("open");
        assert_eq!(db.get_meta("k").expect("get"), None);

        db.set_meta("k", "v1").expect("set");
        assert_eq!(db.get_meta("k").expect("get"), Some("v1".to_string()));

        db.set_meta("k", "v2").expect("set");
        assert_eq!(db.get_meta("k").expect("get"), Some("v2".to_string()));
    }
}
