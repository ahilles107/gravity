//! Durable outbox delivery worker: leases due rows, posts rendered envelopes
//! to each bot session's inbox socket, and records results. At-least-once;
//! consumers deduplicate by delivery id / idempotency key. A bot that is not
//! ready (stopped, socket unknown) defers without consuming retry attempts.

use std::sync::Arc;

use bus::envelope::{render_message, render_routine};
use bus::{DeliveryState, Message};
use chrono::Duration as ChronoDuration;

use crate::app::AppState;
use crate::config::DeliveryConfig;
use crate::db::Db;
use crate::events::{Events, Push};
use crate::supervisor::Supervisor;

/// Ceiling on the wait between attempts at a bot that is not ready. Deliveries
/// are durable, so one that never becomes ready waits at this interval rather
/// than being dropped.
const MAX_NOT_READY_BACKOFF_SECONDS: i64 = 60;

pub struct DeliveryWorker {
    /// For forwarding to linked bots, which runs through the peer link.
    pub app: Arc<AppState>,
    pub db: Db,
    pub supervisor: Supervisor,
    pub events: Events,
    pub cfg: DeliveryConfig,
}

impl DeliveryWorker {
    pub async fn run(self) {
        let mut tick =
            tokio::time::interval(std::time::Duration::from_millis(self.cfg.poll_interval_ms));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if let Err(e) = self.step().await {
                tracing::warn!(error = %e, "delivery worker step failed");
            }
        }
    }

    pub async fn step(&self) -> anyhow::Result<()> {
        self.db.recover_expired_leases()?;
        let due = self.db.lease_due_deliveries(self.cfg.lease_seconds, 20)?;
        for delivery in due {
            self.attempt(&delivery.id).await?;
        }
        Ok(())
    }

    async fn attempt(&self, delivery_id: &str) -> anyhow::Result<()> {
        let Some(delivery) = self.db.get_delivery(delivery_id)? else {
            return Ok(());
        };
        let Some(msg) = self.db.get_message(&delivery.message_id)? else {
            self.db
                .mark_delivery_retry(delivery_id, "message missing", chrono::Utc::now(), 0)?;
            return Ok(());
        };
        let outcome = match self.db.get_bot(&delivery.bot_id)? {
            // A bot that runs on a peer: the peer stores and delivers it.
            Some(target) if target.is_linked() => {
                match crate::peer::forward(&self.app, &target, &msg).await {
                    Ok(()) => Ok(()),
                    Err(crate::peer::ForwardError::Offline) => Err(
                        crate::supervisor::DeliverError::NotReady("peer offline".to_string()),
                    ),
                    Err(crate::peer::ForwardError::Failed(e)) => {
                        Err(crate::supervisor::DeliverError::Failed(e))
                    }
                }
            }
            _ => {
                let text = self.render(delivery_id, &delivery.bot_id, &msg)?;
                self.supervisor.deliver(&delivery.bot_id, &text)
            }
        };

        match outcome {
            Ok(()) => {
                self.db.mark_delivered(delivery_id)?;
                tracing::info!(delivery_id, bot_id = %delivery.bot_id, message_id = %msg.id, "delivered");
            }
            Err(crate::supervisor::DeliverError::NotReady(reason)) => {
                // Transient (bot stopped, socket not reported yet): requeue
                // without consuming a retry attempt. The wait grows with how
                // long the row has been waiting, so a bot mid-start is picked
                // up immediately while one that never comes back is not polled
                // every two seconds forever.
                let waiting = (chrono::Utc::now() - delivery.created_at).num_seconds();
                let backoff = (waiting / 8).clamp(2, MAX_NOT_READY_BACKOFF_SECONDS);
                self.db.defer_delivery(
                    delivery_id,
                    chrono::Utc::now() + ChronoDuration::seconds(backoff),
                )?;
                tracing::debug!(delivery_id, reason, backoff, "delivery deferred");
                return Ok(());
            }
            Err(crate::supervisor::DeliverError::Failed(e)) => {
                let backoff = self.cfg.base_backoff_seconds
                    * 2i64.saturating_pow(delivery.attempt_count.min(8) as u32);
                let next = chrono::Utc::now() + ChronoDuration::seconds(backoff);
                let state = self.db.mark_delivery_retry(
                    delivery_id,
                    &e.to_string(),
                    next,
                    self.cfg.max_attempts,
                )?;
                if state == DeliveryState::Failed {
                    self.events.push(Push::notice(
                        "error",
                        "Delivery failed",
                        format!("Message #{} to bot could not be delivered: {e}", msg.num),
                    ));
                }
                tracing::warn!(delivery_id, error = %e, state = state.as_str(), "delivery attempt failed");
            }
        }
        if let Some(updated) = self.db.get_delivery(delivery_id)? {
            self.events.push(Push::DeliveryUpdate { delivery: updated });
        }
        Ok(())
    }

    /// The envelope a local session reads for `msg`.
    fn render(&self, delivery_id: &str, bot_id: &str, msg: &Message) -> anyhow::Result<String> {
        // A decision notice was already rendered by the registry, which owns
        // its header and its phase. Wrapping it in `[msg #N …]` would bury the
        // one thing that makes it authority: that it comes from the owner.
        Ok(if msg.decision_id.is_some() {
            msg.body.clone()
        } else if msg.sender.kind == bus::SenderKind::Routine {
            let run_id = self.db.run_id_for_delivery(delivery_id)?;
            render_routine(&msg.sender.name, msg.num, run_id.as_deref(), &msg.body)
        } else {
            let ref_num = match &msg.ref_message_id {
                Some(rid) => self.db.get_message(rid)?.map(|m| m.num),
                None => None,
            };
            let task_id = self
                .db
                .open_task_for_message(&msg.id, bot_id)?
                .map(|t| t.id);
            render_message(&self.sender_view(msg)?, ref_num, task_id.as_deref())
        })
    }

    /// A linked sender is named with its machine, so a bot can tell a peer
    /// on the other daemon from a colleague here.
    fn sender_view(&self, msg: &Message) -> anyhow::Result<Message> {
        let mut view = msg.clone();
        let Some(bot_id) = &msg.sender.bot_id else {
            return Ok(view);
        };
        if let Some(peer_id) = self.db.get_bot(bot_id)?.and_then(|b| b.peer_id) {
            if let Some(peer) = self.db.get_peer(&peer_id)? {
                view.sender.name = format!("{} @ {}", msg.sender.name, peer.name);
            }
        }
        Ok(view)
    }
}
