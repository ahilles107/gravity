//! Filing a ruling the owner already gave, relayed by the bot that heard it.

use std::sync::Arc;

use bus::envelope::DecisionPhase;
use bus::*;

use crate::app::AppState;
use crate::db::NewDecision;
use crate::events::Push;

use super::publish::notify_phase;
use super::validate::*;
use super::view::{decision_view, Detail};
use super::{conflict, invalid};

/// File a ruling the owner already gave, at a bot's own terminal.
///
/// Born settled, and honest about it: `answered_by` records which bot relayed
/// it, so a reader can tell a typed ruling from a reported one until the owner
/// confirms it.
pub fn record(
    app: &Arc<AppState>,
    me: &Bot,
    title: &str,
    body: &str,
    ruling_text: &str,
    tags: &[String],
    notify: &[String],
) -> anyhow::Result<Recorded> {
    let title = checked_title(title)?;
    let body = checked_body(body)?;
    let tags = checked_tags(tags)?;
    if ruling_text.trim().is_empty() {
        return Err(invalid(
            "'ruling_text' is required: the owner's words, verbatim — a paraphrase is not a \
             record of what they said",
        ));
    }
    // Everything that can refuse runs before the insert. This used to resolve
    // names inside the notify loop, so an unknown one failed a call that had
    // already filed the record and told half the bots — and the bot, seeing an
    // error, would file it again.
    let notify = resolve_notify(app, me, notify)?;
    let relays = app.db.count_unconfirmed_relays(&me.id)?;
    if relays >= MAX_UNCONFIRMED_RELAYS_PER_BOT {
        return Err(conflict(format!(
            "you have {relays} relayed rulings the owner has not confirmed yet, which is the \
             limit. Ask them to confirm what you have already filed in the Control center \
             before recording more."
        )));
    }
    let decision = app.db.insert_decision(NewDecision {
        project_id: &me.project_id,
        kind: DecisionKind::Decision,
        title: &title,
        body: &body,
        options: &[],
        recommendation: None,
        raised_by_bot_id: &me.id,
        on_behalf_of_bot_id: None,
        origin_chain: String::new().as_str(),
        source_message_id: None,
        source_task_id: None,
        priority: Priority::Normal,
        deadline_at: None,
        supersedes_id: None,
        settled: Some(Ruling {
            option: None,
            text: ruling_text.trim().to_string(),
            reason: None,
            answered_at: now(),
            answered_by: format!("owner-via-bot:{}", me.id),
        }),
    })?;
    app.db.set_decision_tags(&decision.id, &tags, &me.id)?;
    let view = decision_view(&app.db, &decision, Detail::Summary)?;
    let mut notified = Vec::with_capacity(notify.targets.len());
    for target in notify.targets {
        app.db
            .try_record_notification(&decision.id, &target.bot_id, None)?;
        notify_phase(app, &target.bot_id, &view, DecisionPhase::Settled, None)?;
        notified.push(target.name);
    }
    app.events.push(Push::DecisionUpdate {
        decision: Box::new(view.clone()),
    });
    Ok(Recorded {
        decision: view,
        notified,
        skipped: notify.skipped,
    })
}

/// What a `record_decision` filed, and who heard about it.
pub struct Recorded {
    pub decision: DecisionView,
    pub notified: Vec<String>,
    /// Names that matched no live bot in the project. Handed back rather than
    /// raised, the way `publish` does it: a typo in one name is not a reason
    /// to lose the record and the other deliveries.
    pub skipped: Vec<String>,
}

/// A bot to tell, and the name the caller used for it.
struct Target {
    bot_id: String,
    name: String,
}

/// Who a `record_decision` can reach, and who it cannot.
struct Notify {
    targets: Vec<Target>,
    skipped: Vec<String>,
}

/// Turn notify names into live bot ids, before anything is written.
fn resolve_notify(app: &Arc<AppState>, me: &Bot, notify: &[String]) -> anyhow::Result<Notify> {
    if notify.len() > MAX_RECORD_NOTIFY_BOTS {
        return Err(invalid(format!(
            "'notify' names {} bots; at most {MAX_RECORD_NOTIFY_BOTS} — name the ones whose \
             record this ruling contradicts, not the whole project",
            notify.len()
        )));
    }
    let mut targets: Vec<Target> = Vec::new();
    let mut skipped = Vec::new();
    for name in notify {
        match app.db.get_bot_by_name(&me.project_id, name)? {
            Some(bot) if targets.iter().any(|t| t.bot_id == bot.id) => {}
            Some(bot) => targets.push(Target {
                bot_id: bot.id,
                name: name.clone(),
            }),
            None => skipped.push(name.clone()),
        }
    }
    Ok(Notify { targets, skipped })
}
