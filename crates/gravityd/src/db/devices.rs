//! Device credential rows.

use bus::*;
use rusqlite::{params, OptionalExtension, Row};

use super::{parse_ts, ts, Db};

impl Db {
    // ---- devices ----

    fn device_from_row(r: &Row<'_>) -> rusqlite::Result<Device> {
        let caps: String = r.get(2)?;
        Ok(Device {
            id: r.get(0)?,
            name: r.get(1)?,
            capabilities: caps.split(',').filter_map(Capability::parse).collect(),
            created_at: parse_ts(&r.get::<_, String>(3)?),
            revoked_at: r.get::<_, Option<String>>(4)?.map(|s| parse_ts(&s)),
            last_seen_at: r.get::<_, Option<String>>(5)?.map(|s| parse_ts(&s)),
        })
    }

    const DEVICE_COLS: &'static str =
        "id, name, capabilities, created_at, revoked_at, last_seen_at";

    pub fn create_device(&self, name: &str, capabilities: &[Capability]) -> anyhow::Result<Device> {
        let d = Device {
            id: new_id(),
            name: name.to_string(),
            capabilities: capabilities.to_vec(),
            created_at: now(),
            revoked_at: None,
            last_seen_at: None,
        };
        let caps: Vec<&str> = capabilities.iter().map(|c| c.as_str()).collect();
        self.lock().execute(
            "INSERT INTO device(id, name, capabilities, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![d.id, d.name, caps.join(","), ts(d.created_at)],
        )?;
        Ok(d)
    }

    pub fn list_devices(&self) -> anyhow::Result<Vec<Device>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {} FROM device ORDER BY created_at",
            Self::DEVICE_COLS
        ))?;
        let rows = stmt.query_map([], Self::device_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get_device(&self, id: &str) -> anyhow::Result<Option<Device>> {
        let conn = self.lock();
        Ok(conn
            .query_row(
                &format!("SELECT {} FROM device WHERE id = ?1", Self::DEVICE_COLS),
                params![id],
                Self::device_from_row,
            )
            .optional()?)
    }

    pub fn revoke_device(&self, id: &str) -> anyhow::Result<bool> {
        let n = self.lock().execute(
            "UPDATE device SET revoked_at = ?2 WHERE id = ?1 AND revoked_at IS NULL",
            params![id, ts(now())],
        )?;
        Ok(n > 0)
    }

    pub fn touch_device(&self, id: &str) -> anyhow::Result<()> {
        self.lock().execute(
            "UPDATE device SET last_seen_at = ?2 WHERE id = ?1",
            params![id, ts(now())],
        )?;
        Ok(())
    }
}
