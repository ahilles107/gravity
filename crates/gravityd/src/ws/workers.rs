//! A project's temporary workers, for the app: the queue, what runs, what
//! finished, and cancelling a spawn on the owner's behalf.

use serde_json::{json, Value};

use super::Conn;
use crate::messaging::user_sender;
use crate::workers;

/// Finished spawns listed beside the active ones.
const LISTED_FINISHED: i64 = 30;

impl Conn {
    pub(super) fn list_workers(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let project_id = Self::str_field(req, "project_id")?;
        let listed = self
            .app
            .db
            .project_workers(project_id, LISTED_FINISHED)?
            .iter()
            .map(|w| workers::view(&self.app, w))
            .collect::<anyhow::Result<Vec<_>>>()?;
        self.send(json!({
            "type": "workers", "req_id": req_id, "project_id": project_id,
            "workers": listed,
            "running_here": self.app.db.count_live_workers(project_id)?,
            "max_workers_here": self.app.cfg.max_workers_per_project,
        }));
        Ok(())
    }

    /// Cancel a queued or running spawn. A running worker is told to stop by
    /// the owner, and retires.
    pub(super) fn cancel_worker(&self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let worker_id = Self::str_field(req, "worker_id")?;
        let Some(worker) = self.app.db.get_worker(worker_id)? else {
            self.reply_err(req_id, "not_found", "no such worker");
            return Ok(());
        };
        let reason = req.get("reason").and_then(Value::as_str).unwrap_or("");
        match workers::cancel_spawn(&self.app, &worker, &user_sender(), reason) {
            Ok(cancelled) => self.send(json!({
                "type": "worker", "req_id": req_id,
                "worker": workers::view(&self.app, &cancelled)?
            })),
            Err(error) => self.reply_err(req_id, "conflict", &error.to_string()),
        }
        Ok(())
    }
}
