//! The operations on a decision, in one place for all three transports.

use std::sync::Arc;

use bus::envelope::DecisionPhase;
use bus::*;

use crate::app::AppState;
use crate::db::{Actor, DecisionEdit, NewDecision};
use crate::events::Push;

use super::authority::{may, Action};
use super::publish::notify_phase;
use super::validate::*;
use super::view::{decision_view, Detail};
use super::{conflict, forbidden, invalid, not_found};

/// What a bot supplies when it raises a decision.
pub struct RaiseRequest<'a> {
    pub kind: DecisionKind,
    pub title: &'a str,
    pub body: &'a str,
    pub options: &'a [DecisionOption],
    pub recommendation: Option<&'a str>,
    pub tags: &'a [String],
    pub priority: Priority,
    pub deadline_at: Option<&'a str>,
    pub on_behalf_of: Option<&'a str>,
    pub source_task_id: Option<&'a str>,
    pub supersedes: Option<&'a str>,
}

/// A raise, plus what already exists that looks like it.
pub struct Raised {
    pub decision: DecisionView,
    /// Handed back rather than enforced: a settled ruling the bot has not read
    /// is the thing that makes a closed topic come back as a fresh finding.
    pub similar: Vec<DecisionView>,
}

/// File a decision for the owner.
pub fn raise(app: &Arc<AppState>, me: &Bot, req: &RaiseRequest<'_>) -> anyhow::Result<Raised> {
    let title = checked_title(req.title)?;
    let body = checked_body(req.body)?;
    let options = checked_options(req.options)?;
    let recommendation = checked_recommendation(req.recommendation, &options)?;
    let tags = checked_tags(req.tags)?;
    let deadline_at = checked_deadline(req.deadline_at)?;

    // The same bot asking the same thing twice is a bug in the bot, not a new
    // decision — unless it says so by naming what it supersedes.
    let supersedes = checked_supersedes(app, me, req.supersedes)?;
    if supersedes.is_none() {
        if let Some(open) = app.db.open_duplicate(&me.project_id, &me.id, &title)? {
            return Err(conflict(format!(
                "you already have decision {} open with this title, raised {}. Add what is new \
                 with comment_decision, or pass 'supersedes' if the facts have changed.",
                open.id,
                open.created_at.format("%-d %b")
            )));
        }
    }
    let on_behalf_of = match req.on_behalf_of {
        Some(name) => Some(
            app.db
                .get_bot_by_name(&me.project_id, name)?
                .ok_or_else(|| invalid(format!("no bot named '{name}' in this project")))?
                .id,
        ),
        None => None,
    };
    let (hop_chain, source_message_id) = origin(app, &me.id)?;
    let decision = app.db.insert_decision(NewDecision {
        project_id: &me.project_id,
        kind: req.kind,
        title: &title,
        body: &body,
        options: &options,
        recommendation: recommendation.as_deref(),
        raised_by_bot_id: &me.id,
        on_behalf_of_bot_id: on_behalf_of.as_deref(),
        origin_chain: &hop_chain,
        source_message_id: source_message_id.as_deref(),
        source_task_id: req.source_task_id,
        priority: req.priority,
        deadline_at,
        supersedes_id: supersedes.as_deref(),
        settled: None,
    })?;
    app.db.set_decision_tags(&decision.id, &tags, &me.id)?;
    let view = decision_view(&app.db, &decision, Detail::Summary)?;

    tell_the_lead(app, me, &view)?;
    if req.priority == Priority::Urgent {
        app.events.push(Push::decision_notice(
            format!("{} needs a decision", crate::db::Db::display_name(me)),
            view.title.clone(),
            &view.id,
        ));
    }
    app.events.push(Push::DecisionUpdate {
        decision: Box::new(view.clone()),
    });
    let similar = super::view::decision_views(
        &app.db,
        &app.db.near_duplicates(&me.project_id, &title, &tags, 3)?,
    )?
    .into_iter()
    .filter(|d| d.id != view.id)
    .collect();
    Ok(Raised {
        decision: view,
        similar,
    })
}

/// Resolve what a raise claims to supersede.
///
/// Unchecked, this was the one bot-supplied field that wrote to another
/// record: `insert_decision` stamps `superseded_by_id` on whatever id it is
/// given, so a bot could mark a ruling in a project it cannot even read as no
/// longer current. It also skips the duplicate refusal, so it has to be a
/// claim the bot can actually back.
fn checked_supersedes(
    app: &Arc<AppState>,
    me: &Bot,
    supersedes: Option<&str>,
) -> anyhow::Result<Option<String>> {
    let Some(id) = supersedes.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let old = app
        .db
        .get_decision(id)?
        .ok_or_else(|| invalid(format!("no decision with id '{id}' to supersede")))?;
    if old.project_id != me.project_id {
        return Err(forbidden(format!(
            "decision {id} belongs to another project — you cannot supersede it"
        )));
    }
    Ok(Some(old.id))
}

/// The project lead hears about every raise it did not make.
///
/// Not a gate — it cannot answer — but a lead whose teammates ask the owner
/// directly ends up with a stale picture of its own project, which is exactly
/// what the hand-kept ledgers were for.
fn tell_the_lead(app: &Arc<AppState>, me: &Bot, view: &DecisionView) -> anyhow::Result<()> {
    let Some(lead) = app.db.lead_for_project(&me.project_id, &me.id)? else {
        return Ok(());
    };
    if lead == me.id {
        return Ok(());
    }
    notify_phase(app, &lead, view, DecisionPhase::Raised, None)
}

/// Carry the chain of the task the bot is currently working, so the owner can
/// see the ask arrived through Auction → Chief of Staff rather than nowhere.
fn origin(app: &Arc<AppState>, bot_id: &str) -> anyhow::Result<(String, Option<String>)> {
    let Some(task) = app.db.newest_open_task_for(bot_id)? else {
        return Ok((String::new(), None));
    };
    Ok((task.origin_chain.clone(), Some(task.origin_message_id)))
}

/// Add a turn to a decision's thread.
///
/// An owner comment reaches the asker as a bus note; a bot comment raises the
/// owner's unread marker. Either way the exchange stays on the record instead
/// of in a DM nobody will find again.
pub fn comment(
    app: &Arc<AppState>,
    actor: &Actor<'_>,
    decision_id: &str,
    body: &str,
) -> anyhow::Result<DecisionComment> {
    let decision = load(app, decision_id)?;
    may(actor, Action::Comment, &decision)?;
    let body = checked_comment(body)?;
    let comment = app.db.insert_decision_comment(
        decision_id,
        if actor.is_owner() {
            CommentAuthorKind::User
        } else {
            CommentAuthorKind::Bot
        },
        actor.bot_id(),
        &body,
    )?;
    // Built once and used for both the bus note and the push: it is the same
    // record either way, and re-reading it was two extra query batches.
    let view = decision_view(&app.db, &decision, Detail::Summary)?;
    if actor.is_owner() && decision.state.is_pending() {
        notify_phase(
            app,
            &decision.raised_by_bot_id,
            &view,
            DecisionPhase::Comment,
            Some(&body),
        )?;
    }
    let mut named = comment.clone();
    named.author_name = match actor.bot_id() {
        Some(bot_id) => app
            .db
            .get_bot(bot_id)?
            .as_ref()
            .map(crate::db::Db::display_name)
            .unwrap_or_default(),
        None => "you".to_string(),
    };
    app.events.push(Push::DecisionCommentNew {
        comment: named.clone(),
    });
    app.events.push(Push::DecisionUpdate {
        decision: Box::new(view),
    });
    Ok(named)
}

/// Withdraw a decision the asker no longer needs, or the owner has judged is
/// not a decision at all.
pub fn withdraw(
    app: &Arc<AppState>,
    actor: &Actor<'_>,
    decision_id: &str,
    reason: &str,
) -> anyhow::Result<DecisionView> {
    let decision = load(app, decision_id)?;
    may(actor, Action::Withdraw, &decision)?;
    if !app.db.try_withdraw(decision_id, reason.trim())? {
        return Err(conflict(format!(
            "decision {decision_id} is already {}",
            decision.state.as_str()
        )));
    }
    pushed(app, decision_id)
}

pub fn load(app: &Arc<AppState>, decision_id: &str) -> anyhow::Result<Decision> {
    app.db
        .get_decision(decision_id)?
        .ok_or_else(|| not_found("no decision with that id"))
}

/// Re-read, render, push, return — the tail of every mutation.
pub fn pushed(app: &Arc<AppState>, decision_id: &str) -> anyhow::Result<DecisionView> {
    let decision = load(app, decision_id)?;
    let view = decision_view(&app.db, &decision, Detail::Full)?;
    app.events.push(Push::DecisionUpdate {
        decision: Box::new(view.clone()),
    });
    Ok(view)
}

/// Refuse a bot that is trying to do something only the owner may.
pub fn owner_only(actor: &Actor<'_>, what: &str) -> anyhow::Result<()> {
    if actor.is_owner() {
        return Ok(());
    }
    Err(forbidden(format!(
        "only the owner can {what}; a decision you can answer yourself was never a decision"
    )))
}

/// Build the editable face of a decision from its current values, for a
/// caller about to patch some of them.
pub fn current_edit(decision: &Decision) -> DecisionEdit {
    DecisionEdit {
        title: decision.title.clone(),
        body: decision.body.clone(),
        options: decision.options.clone(),
        recommendation: decision.recommendation.clone(),
        priority: decision.priority,
        deadline_at: decision.deadline_at,
        ruling: decision.ruling.clone(),
    }
}
