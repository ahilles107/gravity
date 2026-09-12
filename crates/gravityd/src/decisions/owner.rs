//! The rulings only the owner can make.
//!
//! Answering is deliberately two steps. The owner works through a batch,
//! drafting rulings nobody can see, and publishes once — because the live
//! projects showed the owner answering several questions in one typed message,
//! and a registry that published each keystroke would send a bot half a
//! ruling.

use std::sync::Arc;

use bus::envelope::DecisionPhase;
use bus::*;

use crate::app::AppState;
use crate::db::Actor;
use crate::events::Push;

use super::authority::is_relayed;
use super::publish::notify_phase;
use super::service::{current_edit, load, owner_only, pushed};
use super::validate::*;
use super::view::{decision_view, Detail};
use super::{conflict, invalid, not_found};

/// Draft a ruling. Nothing reaches a bot until it is published.
pub fn answer(
    app: &Arc<AppState>,
    actor: &Actor<'_>,
    decision_id: &str,
    option: Option<&str>,
    text: &str,
    reason: Option<&str>,
) -> anyhow::Result<DecisionView> {
    owner_only(actor, "answer a decision")?;
    let decision = load(app, decision_id)?;
    let text = text.trim();
    if text.is_empty() {
        return Err(invalid(
            "'ruling_text' is required — bots quote your words, so a ruling with none is an \
             option key nobody can act on",
        ));
    }
    let option = checked_ruling_option(option, &decision.options)?;
    let ruling = Ruling {
        option,
        text: text.to_string(),
        reason: reason
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(String::from),
        answered_at: now(),
        answered_by: actor_label(actor),
    };
    if !app.db.try_answer(decision_id, &ruling)? {
        return Err(conflict(format!(
            "decision {decision_id} is {} and cannot be answered",
            decision.state.as_str()
        )));
    }
    pushed(app, decision_id)
}

pub fn unanswer(
    app: &Arc<AppState>,
    actor: &Actor<'_>,
    decision_id: &str,
) -> anyhow::Result<DecisionView> {
    owner_only(actor, "discard a draft ruling")?;
    if !app.db.try_unanswer(decision_id)? {
        return Err(conflict("that decision has no draft ruling to discard"));
    }
    pushed(app, decision_id)
}

/// Park a decision, optionally with a date and a question.
///
/// The asker is told, so it stops waiting on an answer today. This is what
/// replaces "held at your instruction" carried forward in prompt text.
pub fn hold(
    app: &Arc<AppState>,
    actor: &Actor<'_>,
    decision_id: &str,
    until: Option<&str>,
    comment: Option<&str>,
) -> anyhow::Result<DecisionView> {
    owner_only(actor, "hold a decision")?;
    let decision = load(app, decision_id)?;
    let until = checked_deadline(until)?;
    if !app.db.try_hold(decision_id, until)? {
        return Err(conflict(format!(
            "decision {decision_id} is {} and cannot be held",
            decision.state.as_str()
        )));
    }
    let note = comment.map(str::trim).filter(|c| !c.is_empty());
    if let Some(note) = note {
        app.db
            .insert_decision_comment(decision_id, CommentAuthorKind::User, None, note)?;
    }
    let refreshed = load(app, decision_id)?;
    let view = decision_view(&app.db, &refreshed, Detail::Summary)?;
    notify_phase(
        app,
        &refreshed.raised_by_bot_id,
        &view,
        DecisionPhase::Held,
        note,
    )?;
    pushed(app, decision_id)
}

pub fn resume(
    app: &Arc<AppState>,
    actor: &Actor<'_>,
    decision_id: &str,
) -> anyhow::Result<DecisionView> {
    owner_only(actor, "resume a decision")?;
    if !app.db.try_resume(decision_id)? {
        return Err(conflict("that decision is not held"));
    }
    let refreshed = load(app, decision_id)?;
    let view = decision_view(&app.db, &refreshed, Detail::Summary)?;
    notify_phase(
        app,
        &refreshed.raised_by_bot_id,
        &view,
        DecisionPhase::Resumed,
        None,
    )?;
    pushed(app, decision_id)
}

/// Confirm a ruling a bot relayed, turning "Argus says Paweł said yes" into
/// the owner's own word.
pub fn confirm(
    app: &Arc<AppState>,
    actor: &Actor<'_>,
    decision_id: &str,
) -> anyhow::Result<DecisionView> {
    owner_only(actor, "confirm a relayed ruling")?;
    let decision = load(app, decision_id)?;
    if !is_relayed(&decision) {
        return Err(conflict(
            "that decision was not relayed by a bot, so there is nothing to confirm",
        ));
    }
    if !app.db.try_confirm(decision_id)? {
        return Err(conflict("that ruling is already the owner's own"));
    }
    let refreshed = load(app, decision_id)?;
    let view = decision_view(&app.db, &refreshed, Detail::Summary)?;
    for bot in app.db.list_notifications(decision_id)? {
        notify_phase(app, &bot.bot_id, &view, DecisionPhase::Settled, None)?;
    }
    pushed(app, decision_id)
}

/// A settled topic that has come back is a new decision on current facts, not
/// an edit of the old one — which is the rule the lead bots already follow.
pub fn reopen(
    app: &Arc<AppState>,
    actor: &Actor<'_>,
    decision_id: &str,
    title: Option<&str>,
    body: Option<&str>,
) -> anyhow::Result<DecisionView> {
    owner_only(actor, "reopen a decision")?;
    let old = load(app, decision_id)?;
    if old.state != DecisionState::Settled {
        return Err(conflict("only a settled decision can be reopened"));
    }
    let tags = app
        .db
        .tags_for(std::slice::from_ref(&old.id))?
        .remove(&old.id)
        .unwrap_or_default();
    let new = app.db.insert_decision(crate::db::NewDecision {
        project_id: &old.project_id,
        kind: old.kind,
        title: &checked_title(title.unwrap_or(&old.title))?,
        body: &checked_body(body.unwrap_or(&old.body))?,
        options: &old.options,
        recommendation: old.recommendation.as_deref(),
        raised_by_bot_id: &old.raised_by_bot_id,
        on_behalf_of_bot_id: old.on_behalf_of_bot_id.as_deref(),
        origin_chain: &old.origin_chain,
        source_message_id: None,
        source_task_id: None,
        priority: old.priority,
        deadline_at: None,
        supersedes_id: Some(&old.id),
        settled: None,
    })?;
    app.db.set_decision_tags(&new.id, &tags, "user")?;
    pushed(app, &old.id)?;
    pushed(app, &new.id)
}

/// Delete a record outright. Everything else in this module is reversible;
/// this one is confirmed in the client for that reason.
pub fn delete(app: &Arc<AppState>, actor: &Actor<'_>, decision_id: &str) -> anyhow::Result<()> {
    owner_only(actor, "delete a decision")?;
    if !app.db.delete_decision(decision_id)? {
        return Err(not_found("no decision with that id"));
    }
    app.events.push(Push::DecisionDeleted {
        decision_id: decision_id.to_string(),
    });
    Ok(())
}

/// Replace a decision's tags.
pub fn set_tags(
    app: &Arc<AppState>,
    actor: &Actor<'_>,
    decision_id: &str,
    tags: &[String],
) -> anyhow::Result<DecisionView> {
    let decision = load(app, decision_id)?;
    super::may(actor, super::Action::Retag, &decision)?;
    let tags = checked_tags(tags)?;
    app.db
        .set_decision_tags(decision_id, &tags, &actor.as_stored())?;
    pushed(app, decision_id)
}

/// Patch any field the owner may change, including a settled ruling.
pub struct Patch<'a> {
    pub title: Option<&'a str>,
    pub body: Option<&'a str>,
    pub options: Option<&'a [DecisionOption]>,
    pub recommendation: Option<&'a str>,
    pub priority: Option<Priority>,
    /// `Some(None)` clears the deadline; `None` leaves it alone.
    pub deadline_at: Option<Option<&'a str>>,
    pub ruling_option: Option<&'a str>,
    pub ruling_text: Option<&'a str>,
    pub ruling_reason: Option<&'a str>,
}

pub fn update(
    app: &Arc<AppState>,
    actor: &Actor<'_>,
    decision_id: &str,
    patch: &Patch<'_>,
) -> anyhow::Result<DecisionView> {
    owner_only(actor, "edit a decision")?;
    let decision = load(app, decision_id)?;
    let mut edit = current_edit(&decision);
    if let Some(title) = patch.title {
        edit.title = checked_title(title)?;
    }
    if let Some(body) = patch.body {
        edit.body = checked_body(body)?;
    }
    if let Some(options) = patch.options {
        edit.options = checked_options(options)?;
    }
    if let Some(recommendation) = patch.recommendation {
        edit.recommendation = checked_recommendation(Some(recommendation), &edit.options)?;
    }
    if let Some(priority) = patch.priority {
        edit.priority = priority;
    }
    if let Some(deadline) = patch.deadline_at {
        edit.deadline_at = checked_deadline(deadline)?;
    }
    apply_ruling_patch(&mut edit, patch, actor)?;
    app.db
        .update_decision(decision_id, &edit, &actor.as_stored())?;
    pushed(app, decision_id)
}

/// Editing the ruling of a decision that has none is how an owner publishes in
/// one step, so the ruling fields are patched together rather than one at a
/// time — a reason without words would publish an empty ruling.
fn apply_ruling_patch(
    edit: &mut crate::db::DecisionEdit,
    patch: &Patch<'_>,
    actor: &Actor<'_>,
) -> anyhow::Result<()> {
    if patch.ruling_option.is_none() && patch.ruling_text.is_none() && patch.ruling_reason.is_none()
    {
        return Ok(());
    }
    let existing = edit.ruling.clone();
    let text = patch
        .ruling_text
        .map(str::trim)
        .map(String::from)
        .or_else(|| existing.as_ref().map(|r| r.text.clone()))
        .filter(|t| !t.is_empty())
        .ok_or_else(|| invalid("a ruling needs words; bots quote them"))?;
    edit.ruling = Some(Ruling {
        option: checked_ruling_option(patch.ruling_option, &edit.options)?
            .or_else(|| existing.as_ref().and_then(|r| r.option.clone())),
        text,
        reason: patch
            .ruling_reason
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(String::from)
            .or_else(|| existing.as_ref().and_then(|r| r.reason.clone())),
        answered_at: existing.as_ref().map(|r| r.answered_at).unwrap_or_else(now),
        answered_by: existing
            .as_ref()
            .map(|r| r.answered_by.clone())
            .unwrap_or_else(|| actor_label(actor)),
    });
    Ok(())
}

/// How a ruling records who gave it. A device is still the owner, but a
/// revoked device should stay attributable.
fn actor_label(actor: &Actor<'_>) -> String {
    match actor {
        Actor::Device(id) => format!("device:{id}"),
        _ => "owner".to_string(),
    }
}
