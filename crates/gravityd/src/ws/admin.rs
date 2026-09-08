//! Delivery, search, device and diagnostics requests.

use bus::{Capability, DeliveryState};
use serde_json::{json, Value};

use crate::app::{DAEMON_VERSION, PROTOCOL_VERSION};
use crate::config::{AUTO_COMPACT_WINDOW_MAX, AUTO_COMPACT_WINDOW_MIN};
use crate::events::Push;

use super::Conn;

impl Conn {
    // ---- deliveries / search / diagnostics ----

    pub(super) fn list_deliveries(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = req.get("bot_id").and_then(|v| v.as_str());
        let state = req
            .get("state")
            .and_then(|v| v.as_str())
            .and_then(DeliveryState::parse);
        let deliveries = self.app.db.list_deliveries(bot_id, state)?;
        self.send(json!({ "type": "deliveries", "req_id": req_id, "deliveries": deliveries }));
        Ok(())
    }

    pub(super) fn retry_delivery(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let delivery_id = Self::str_field(req, "delivery_id")?;
        if self.app.db.retry_failed_delivery(delivery_id)? {
            if let Some(d) = self.app.db.get_delivery(delivery_id)? {
                self.app.events.push(Push::DeliveryUpdate { delivery: d });
            }
            self.send(json!({ "type": "ok", "req_id": req_id }));
        } else {
            self.reply_err(req_id, "conflict", "delivery is not in failed state");
        }
        Ok(())
    }

    pub(super) fn search(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let query = Self::str_field(req, "query")?;
        let results = self.app.db.search_messages(query, 50)?;
        self.send(json!({ "type": "search_results", "req_id": req_id, "search_results": results }));
        Ok(())
    }

    // ---- devices ----

    pub(super) fn list_devices(&self, req_id: &Value) -> anyhow::Result<()> {
        let devices = self.app.db.list_devices()?;
        self.send(json!({ "type": "devices", "req_id": req_id, "devices": devices }));
        Ok(())
    }

    pub(super) fn create_device(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let name = Self::str_field(req, "name")?;
        let caps: Vec<Capability> = req
            .get("capabilities")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().and_then(Capability::parse))
                    .collect()
            })
            .unwrap_or_else(|| vec![Capability::Read]);
        if caps.is_empty() {
            self.reply_err(req_id, "invalid_request", "capabilities must be non-empty");
            return Ok(());
        }
        let device = self.app.db.create_device(name, &caps)?;
        let token = self.app.secrets.issue_device_token(&device.id)?;
        // The token appears in this reply only; it is never listed again.
        self.send(json!({
            "type": "device", "req_id": req_id, "device": device, "token": token
        }));
        Ok(())
    }

    pub(super) fn revoke_device(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let device_id = Self::str_field(req, "device_id")?;
        if !self.app.db.revoke_device(device_id)? {
            self.reply_err(req_id, "not_found", "device not found or already revoked");
            return Ok(());
        }
        self.app.secrets.remove_device_token(device_id)?;
        let device = self
            .app
            .db
            .get_device(device_id)?
            .ok_or_else(|| anyhow::anyhow!("device vanished"))?;
        self.send(json!({ "type": "device", "req_id": req_id, "device": device }));
        Ok(())
    }

    // ---- config ----

    fn config_json(&self) -> Value {
        json!({
            "bind": self.app.cfg.bind,
            "port": self.app.cfg.port,
            // Differs from `port` only when the configured one was occupied at
            // startup, which the client surfaces: the bus moved with it.
            "configured_port": self.app.cfg.configured_port,
            "runtime": self.app.cfg.runtime,
            "auto_compact_window": self.app.auto_compact.effective(&self.app.cfg),
        })
    }

    pub(super) fn get_config(&self, req_id: &Value) -> anyhow::Result<()> {
        self.send(json!({ "type": "config", "req_id": req_id, "config": self.config_json() }));
        Ok(())
    }

    /// Updates `auto_compact_window` (`null` = model default) and persists it
    /// in the `meta` table; bots pick the new value up on their next start.
    /// Everything else in the config describes how the daemon was launched
    /// and stays read-only here.
    pub(super) fn set_config(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let Some(field) = req.get("auto_compact_window") else {
            self.reply_err(
                req_id,
                "invalid_request",
                "'auto_compact_window' is required",
            );
            return Ok(());
        };
        let window = match field {
            Value::Null => None,
            Value::Number(n) => match n.as_u64().and_then(|w| u32::try_from(w).ok()) {
                Some(w) if (AUTO_COMPACT_WINDOW_MIN..=AUTO_COMPACT_WINDOW_MAX).contains(&w) => {
                    Some(w)
                }
                _ => {
                    self.reply_err(
                        req_id,
                        "invalid_request",
                        &format!(
                            "auto_compact_window must be {AUTO_COMPACT_WINDOW_MIN}-{AUTO_COMPACT_WINDOW_MAX} or null"
                        ),
                    );
                    return Ok(());
                }
            },
            _ => {
                self.reply_err(
                    req_id,
                    "invalid_request",
                    "auto_compact_window must be a number or null",
                );
                return Ok(());
            }
        };
        self.app.auto_compact.persist(&self.app.db, window)?;
        self.get_config(req_id)
    }

    pub(super) fn diagnostics(&self, req_id: &Value) -> anyhow::Result<()> {
        let runtime = self.app.supervisor.adapter();
        let caps = runtime.capabilities();
        let (available, version) = match runtime.probe() {
            Ok(v) => (true, Some(v)),
            Err(_) => (false, None),
        };
        let db_healthy = self.app.db.delivery_backlog().is_ok();
        let backlog = self.app.db.delivery_backlog().unwrap_or(-1);
        self.send(json!({
            "type": "diagnostics", "req_id": req_id,
            "diagnostics": {
                "daemon_version": DAEMON_VERSION,
                "protocol_version": PROTOCOL_VERSION,
                "stale_build": self.app.stale_build(),
                "db_healthy": db_healthy,
                "runtime": { "kind": caps.kind, "available": available, "version": version,
                             "capabilities": caps },
                "delivery_backlog": backlog,
                "active_bots": self.app.supervisor.active_total(),
                "uptime_seconds": self.app.started_at.elapsed().as_secs()
            }
        }));
        Ok(())
    }
}
