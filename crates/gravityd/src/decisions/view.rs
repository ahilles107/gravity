//! The one renderer.
//!
//! `bot_view` earned this rule the hard way: a stored row handed straight to a
//! client is missing everything that lives in another table, and a reply and a
//! push that build it separately drift. So every path that hands a decision to
//! anyone — a WebSocket reply, a push, an MCP result, a rendered envelope —
//! comes through here.

use std::collections::HashMap;

use bus::*;

use crate::db::Db;

/// How much of a decision to assemble. A list of fifty does not need fifty
/// comment threads; a detail view does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    Summary,
    Full,
}

/// Assemble one decision for a caller.
pub fn decision_view(db: &Db, decision: &Decision, detail: Detail) -> anyhow::Result<DecisionView> {
    let ids = std::slice::from_ref(&decision.id);
    let comments = match detail {
        Detail::Full => db.list_decision_comments(&decision.id)?,
        Detail::Summary => Vec::new(),
    };
    let bots = db.bots_by_id(&bot_ids(std::slice::from_ref(decision), &comments))?;
    let mut view = base_view(decision, &bots);
    view.tags = db.tags_for(ids)?.remove(&decision.id).unwrap_or_default();
    if let Some((count, last)) = db.comment_counts(ids)?.get(&decision.id) {
        view.comment_count = *count;
        view.last_comment_at = Some(parse_rfc3339(last));
    }
    if detail == Detail::Full {
        view.comments = named_comments(&comments, &bots);
        view.notifications = db.list_notifications(&decision.id)?;
    }
    Ok(view)
}

/// The row plus its raiser, from an already-resolved batch of bots.
fn base_view(decision: &Decision, bots: &HashMap<String, Bot>) -> DecisionView {
    let raiser = bots.get(&decision.raised_by_bot_id);
    DecisionView {
        id: decision.id.clone(),
        project_id: decision.project_id.clone(),
        kind: decision.kind,
        title: decision.title.clone(),
        body: decision.body.clone(),
        options: decision.options.clone(),
        recommendation: decision.recommendation.clone(),
        raised_by: RaisedBy {
            bot_id: decision.raised_by_bot_id.clone(),
            name: raiser
                .map(Db::display_name)
                .unwrap_or_else(|| "a deleted bot".to_string()),
            avatar: raiser.map(|b| b.avatar.clone()).unwrap_or_default(),
        },
        on_behalf_of_bot_id: decision.on_behalf_of_bot_id.clone(),
        origin_chain: decision.origin_chain.clone(),
        source_message_id: decision.source_message_id.clone(),
        source_task_id: decision.source_task_id.clone(),
        priority: decision.priority,
        deadline_at: decision.deadline_at,
        state: decision.state,
        held_until: decision.held_until,
        ruling: decision.ruling.clone(),
        published_at: decision.published_at,
        supersedes_id: decision.supersedes_id.clone(),
        superseded_by_id: decision.superseded_by_id.clone(),
        withdrawn_reason: decision.withdrawn_reason.clone(),
        tags: Vec::new(),
        comment_count: 0,
        last_comment_at: None,
        comments: Vec::new(),
        notifications: Vec::new(),
        edited_at: decision.edited_at,
        created_at: decision.created_at,
    }
}

/// Every bot id a render will need a name for.
fn bot_ids(decisions: &[Decision], comments: &[DecisionComment]) -> Vec<String> {
    let mut ids: Vec<String> = decisions
        .iter()
        .map(|d| d.raised_by_bot_id.clone())
        .chain(comments.iter().filter_map(|c| c.author_bot_id.clone()))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Assemble a page of decisions without a query per row.
pub fn decision_views(db: &Db, decisions: &[Decision]) -> anyhow::Result<Vec<DecisionView>> {
    let ids: Vec<String> = decisions.iter().map(|d| d.id.clone()).collect();
    let mut tags = db.tags_for(&ids)?;
    let counts = db.comment_counts(&ids)?;
    let bots = db.bots_by_id(&bot_ids(decisions, &[]))?;
    let mut out = Vec::with_capacity(decisions.len());
    for decision in decisions {
        let mut view = base_view(decision, &bots);
        view.tags = tags.remove(&decision.id).unwrap_or_default();
        if let Some((count, last)) = counts.get(&decision.id) {
            view.comment_count = *count;
            view.last_comment_at = Some(parse_rfc3339(last));
        }
        out.push(view);
    }
    Ok(out)
}

/// Resolve author names at render time. A bot may rename itself between
/// writing a comment and the owner reading it, and the owner should see who
/// the bot is now.
fn named_comments(
    comments: &[DecisionComment],
    bots: &HashMap<String, Bot>,
) -> Vec<DecisionComment> {
    comments
        .iter()
        .map(|comment| {
            let mut comment = comment.clone();
            comment.author_name = match &comment.author_bot_id {
                Some(bot_id) => bots
                    .get(bot_id)
                    .map(Db::display_name)
                    .unwrap_or_else(|| "a deleted bot".to_string()),
                None => "you".to_string(),
            };
            comment
        })
        .collect()
}

fn parse_rfc3339(value: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| now())
}
