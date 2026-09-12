//! The decision-registry tools, as bots call them.
//!
//! Thin on purpose: parse arguments, call the service, serialise. Every rule
//! about who may do what, and every state transition, lives in
//! `crate::decisions` so the control plane cannot answer the same question
//! differently.

use std::sync::Arc;

use bus::*;
use serde_json::{json, Value};

use crate::app::AppState;
use crate::db::{Actor, DecisionFilter};
use crate::decisions::{self, Detail, RaiseRequest};

use super::caller;

/// The calling bot as an actor. Both halves come from the bearer token.
fn actor<'a>(me: &'a Bot, bot_id: &'a str) -> Actor<'a> {
    Actor::Bot {
        id: bot_id,
        project_id: &me.project_id,
    }
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

fn required<'a>(args: &'a Value, key: &str) -> anyhow::Result<&'a str> {
    str_arg(args, key).ok_or_else(|| anyhow::anyhow!("'{key}' is required"))
}

fn string_list(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn raise_decision(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let options: Vec<DecisionOption> = match args.get("options") {
        Some(value) if !value.is_null() => serde_json::from_value(value.clone())
            .map_err(|e| anyhow::anyhow!("'options' is not a list of {{key, label}}: {e}"))?,
        _ => Vec::new(),
    };
    let tags = string_list(args, "tags");
    let raised = decisions::raise(
        app,
        &me,
        &RaiseRequest {
            kind: str_arg(args, "kind")
                .and_then(DecisionKind::parse)
                .unwrap_or(DecisionKind::Decision),
            title: required(args, "title")?,
            body: required(args, "body")?,
            options: &options,
            recommendation: str_arg(args, "recommendation"),
            tags: &tags,
            priority: str_arg(args, "priority")
                .and_then(Priority::parse)
                .unwrap_or(Priority::Normal),
            deadline_at: str_arg(args, "deadline_at"),
            on_behalf_of: str_arg(args, "on_behalf_of"),
            source_task_id: str_arg(args, "source_task_id"),
            supersedes: str_arg(args, "supersedes"),
        },
    )?;
    let mut out = json!({ "decision": summary(&raised.decision) });
    if !raised.similar.is_empty() {
        // Advisory, and worded as such: the point is that the bot reads a
        // settled ruling before re-opening the topic, not that it is blocked.
        out["similar"] = json!(raised.similar.iter().map(summary).collect::<Vec<_>>());
        out["similar_note"] = json!(
            "Decisions in this project that look related. If one of these already settles \
             your question, act on it instead."
        );
    }
    Ok(out)
}

pub(super) fn list_decisions(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let states: Vec<DecisionState> = match str_arg(args, "state").unwrap_or("open") {
        "settled" => vec![DecisionState::Settled],
        "all" => Vec::new(),
        _ => vec![
            DecisionState::Open,
            DecisionState::Answered,
            DecisionState::Held,
        ],
    };
    // One query per tag below, so the same ceiling that bounds a decision's
    // tags bounds how many passes a caller can ask for.
    let tags = decisions::checked_tags(&string_list(args, "tags"))?;
    let mine = args
        .get("mine")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        .then_some(bot_id);
    let limit = args
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(20)
        .clamp(1, 100);
    let mut decisions_out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    // One pass per tag rather than an IN clause: a decision tagged both is
    // still one result, and the shared taxonomy makes multi-tag rare.
    for tag in tags.iter().map(Some).chain(tags.is_empty().then_some(None)) {
        let found = app.db.list_decisions(&DecisionFilter {
            project_id: Some(&me.project_id),
            states: &states,
            tag: tag.map(String::as_str),
            bot_id: mine,
            query: str_arg(args, "query"),
            limit,
            ..Default::default()
        })?;
        for decision in found {
            if seen.insert(decision.id.clone()) {
                decisions_out.push(decision);
            }
        }
    }
    let views = decisions::decision_views(&app.db, &decisions_out)?;
    Ok(json!({ "decisions": views.iter().map(summary).collect::<Vec<_>>() }))
}

pub(super) fn get_decision(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let decision = decisions::load(app, required(args, "id")?)?;
    if decision.project_id != me.project_id {
        anyhow::bail!("that decision belongs to another project");
    }
    let view = decisions::decision_view(&app.db, &decision, Detail::Full)?;
    Ok(json!({ "decision": view }))
}

pub(super) fn comment_decision(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let comment = decisions::comment(
        app,
        &actor(&me, bot_id),
        required(args, "id")?,
        required(args, "body")?,
    )?;
    Ok(json!({ "comment": comment }))
}

pub(super) fn withdraw_decision(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let view = decisions::withdraw(
        app,
        &actor(&me, bot_id),
        required(args, "id")?,
        required(args, "reason")?,
    )?;
    Ok(json!({ "decision": summary(&view) }))
}

pub(super) fn record_decision(
    app: &Arc<AppState>,
    bot_id: &str,
    args: &Value,
) -> anyhow::Result<Value> {
    let me = caller(app, bot_id)?;
    let recorded = decisions::record(
        app,
        &me,
        required(args, "title")?,
        required(args, "body")?,
        required(args, "ruling_text")?,
        &string_list(args, "tags"),
        &string_list(args, "notify"),
    )?;
    let mut out = json!({
        "decision": summary(&recorded.decision),
        "notified": recorded.notified,
        "note": "Recorded as relayed by you. The owner can confirm it in the Control center; \
                 until then other bots should treat it as a relay with a paper trail."
    });
    if !recorded.skipped.is_empty() {
        // Named, not swallowed: the record is filed either way, and a bot that
        // believes it warned a colleague it did not is the failure this whole
        // tool exists to avoid.
        out["not_notified"] = json!(recorded.skipped);
        out["not_notified_note"] = json!(
            "No live bot in this project has these names. The ruling is filed; tell them \
                   with send_message if they still need to know."
        );
    }
    Ok(out)
}

/// The shape a bot reads: everything it needs to act, without the body of
/// every record in a list eating its context window.
fn summary(view: &DecisionView) -> Value {
    let mut out = json!({
        "id": view.id,
        "title": view.title,
        "state": view.state.as_str(),
        "priority": view.priority.as_str(),
        "raised_by": view.raised_by.name,
        "tags": view.tags,
        "created_at": view.created_at.to_rfc3339()
    });
    if let Some(deadline) = view.deadline_at {
        out["deadline_at"] = json!(deadline.to_rfc3339());
    }
    if let Some(recommendation) = &view.recommendation {
        out["recommendation"] = json!(recommendation);
    }
    if let Some(ruling) = &view.ruling {
        out["ruling"] = json!({
            "text": ruling.text,
            "option": ruling.option,
            "reason": ruling.reason,
            "answered_by": ruling.answered_by
        });
    }
    if view.comment_count > 0 {
        out["comment_count"] = json!(view.comment_count);
    }
    out
}
