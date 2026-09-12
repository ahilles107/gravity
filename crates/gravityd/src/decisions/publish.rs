//! Turning a ruling into deliveries.
//!
//! This is the step the whole feature exists for. A ruling published here
//! reaches a working bot as a bus message the daemon authenticated, sent by
//! the user, carrying the owner's verbatim words — not as a peer saying "Paweł
//! said yes", which the bots have correctly learned to refuse.

use std::sync::Arc;

use bus::envelope::{render_decision, DecisionPhase};
use bus::*;

use crate::app::AppState;
use crate::events::Push;
use crate::messaging::{self, user_sender, Dm};

use super::view::{decision_view, Detail};

/// Who heard about a published decision, and who could not be told.
pub struct PublishOutcome {
    pub decision: DecisionView,
    pub notified: Vec<String>,
    /// Bot name and why, so the client can say "Auction was deleted" rather
    /// than quietly notifying fewer bots than the owner ticked.
    pub skipped: Vec<(String, String)>,
}

/// Settle a decision and tell the bots that need to act on it.
///
/// `notify` is bot ids; `None` means the default set — whoever asked, plus the
/// bot they asked on behalf of. Re-publishing is safe: a bot already recorded
/// as notified is not told twice.
pub fn publish(
    app: &Arc<AppState>,
    decision_id: &str,
    notify: Option<&[String]>,
) -> anyhow::Result<PublishOutcome> {
    let decision = app
        .db
        .get_decision(decision_id)?
        .ok_or_else(|| super::not_found("no decision with that id"))?;
    if decision.state == DecisionState::Settled {
        // Already settled: fall through to notifying, so a publish that
        // half-succeeded can be retried without losing the rest.
    } else if !app.db.try_publish(decision_id)? {
        return Err(super::conflict(format!(
            "decision {decision_id} cannot be published from '{}' — it needs a ruling first",
            decision.state.as_str()
        )));
    }
    let decision = app
        .db
        .get_decision(decision_id)?
        .ok_or_else(|| super::not_found("no decision with that id"))?;
    let view = decision_view(&app.db, &decision, Detail::Summary)?;
    let targets = match notify {
        Some(ids) => ids.to_vec(),
        None => default_notify(&decision),
    };
    let body = render_decision(DecisionPhase::Settled, &view, None);
    let mut outcome = PublishOutcome {
        decision: view,
        notified: Vec::new(),
        skipped: Vec::new(),
    };
    for bot_id in targets {
        match deliver(app, &decision, bot_id.as_str(), &body)? {
            Ok(name) => outcome.notified.push(name),
            Err(skip) => outcome.skipped.push(skip),
        }
    }
    app.events.push(Push::DecisionUpdate {
        decision: Box::new(outcome.decision.clone()),
    });
    Ok(outcome)
}

/// Tell one bot. `Ok(name)` when it was told, `Err((name, reason))` when it
/// could not be.
fn deliver(
    app: &Arc<AppState>,
    decision: &Decision,
    bot_id: &str,
    body: &str,
) -> anyhow::Result<Result<String, (String, String)>> {
    let Some(bot) = app.db.get_bot(bot_id)? else {
        return Ok(Err((bot_id.to_string(), "no such bot".to_string())));
    };
    let name = crate::db::Db::display_name(&bot);
    if bot.deleted_at.is_some() {
        return Ok(Err((name, "the bot was deleted".to_string())));
    }
    if bot.project_id != decision.project_id {
        return Ok(Err((name, "the bot is in another project".to_string())));
    }
    // Already told. Not an error: the owner may be republishing a batch that
    // partly went out, and a bot should hear one ruling once.
    if !app
        .db
        .try_record_notification(&decision.id, &bot.id, None)?
    {
        return Ok(Ok(name));
    }
    // Deliberately not gated on the bot being up: the bus is durable, and a
    // stopped bot reads its ruling when it starts. Only an archived bot is
    // genuinely unreachable.
    let sender = user_sender();
    messaging::send_dm(
        &app.db,
        &app.events,
        Dm::new(&bot.id, &sender, MessageKind::Note, body).about(&decision.id),
    )?;
    Ok(Ok(name))
}

/// Whoever asked, plus whoever is waiting on the answer.
pub fn default_notify(decision: &Decision) -> Vec<String> {
    let mut ids = vec![decision.raised_by_bot_id.clone()];
    if let Some(behalf) = &decision.on_behalf_of_bot_id {
        if behalf != &decision.raised_by_bot_id {
            ids.push(behalf.clone());
        }
    }
    ids
}

/// Send one bot a decision notice that is not a ruling: a raise the lead
/// should know about, an owner comment, a hold, a resume.
pub fn notify_phase(
    app: &Arc<AppState>,
    bot_id: &str,
    view: &DecisionView,
    phase: DecisionPhase,
    note: Option<&str>,
) -> anyhow::Result<()> {
    let Some(bot) = app.db.get_bot(bot_id)? else {
        return Ok(());
    };
    if bot.deleted_at.is_some() {
        return Ok(());
    }
    let body = render_decision(phase, view, note);
    let sender = user_sender();
    messaging::send_dm(
        &app.db,
        &app.events,
        Dm::new(&bot.id, &sender, MessageKind::Note, &body).about(&view.id),
    )?;
    Ok(())
}
