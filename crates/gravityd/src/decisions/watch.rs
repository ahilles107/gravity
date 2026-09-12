//! The clock the registry needs.
//!
//! Two things in the design happen at a time rather than on a request: a
//! deadline coming within a day, and a held decision whose "later" has
//! arrived. Neither has a caller to hang off, and both fail silently if
//! nothing owns them — a held decision that never comes back is the same
//! failure as the questions carried forward in prompt text forever.

use std::sync::Arc;

use bus::envelope::DecisionPhase;

use crate::app::AppState;
use crate::events::Push;

use super::publish::notify_phase;
use super::view::{decision_view, Detail};

/// How far ahead a deadline is worth warning about.
const DUE_SOON_HOURS: i64 = 24;

/// How often to sweep. Minutes rather than seconds: nothing here is urgent to
/// the minute, and a deadline warning is about a day, not a moment.
const SWEEP_INTERVAL_SECONDS: u64 = 300;

pub async fn run_watch(app: Arc<AppState>) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(SWEEP_INTERVAL_SECONDS));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        if let Err(e) = sweep(&app) {
            tracing::warn!(error = %e, "decision sweep failed");
        }
    }
}

fn sweep(app: &Arc<AppState>) -> anyhow::Result<()> {
    resume_held(app)?;
    warn_on_deadlines(app)
}

/// Bring back what the owner parked until now.
///
/// The asker is told, because a decision that quietly reappears on the owner's
/// list while the bot still believes it is held is the stale-ledger problem
/// again, just in the other direction.
fn resume_held(app: &Arc<AppState>) -> anyhow::Result<()> {
    for decision in app.db.held_expired()? {
        if !app.db.try_resume(&decision.id)? {
            continue;
        }
        let refreshed = app
            .db
            .get_decision(&decision.id)?
            .unwrap_or_else(|| decision.clone());
        let view = decision_view(&app.db, &refreshed, Detail::Summary)?;
        notify_phase(
            app,
            &refreshed.raised_by_bot_id,
            &view,
            DecisionPhase::Resumed,
            None,
        )?;
        app.events.push(Push::DecisionUpdate {
            decision: Box::new(view),
        });
    }
    Ok(())
}

/// Warn once per decision, the day its deadline lands.
///
/// Once, not every sweep: the live projects produced the opposite failure, an
/// item restated twenty-seven times until the owner asked it to stop.
fn warn_on_deadlines(app: &Arc<AppState>) -> anyhow::Result<()> {
    for decision in app.db.decisions_due_within(DUE_SOON_HOURS)? {
        if !app.db.try_mark_deadline_notified(&decision.id)? {
            continue;
        }
        app.events.push(Push::decision_notice(
            "A decision is due",
            decision.title.clone(),
            &decision.id,
        ));
        let view = decision_view(&app.db, &decision, Detail::Summary)?;
        app.events.push(Push::DecisionUpdate {
            decision: Box::new(view),
        });
    }
    Ok(())
}
