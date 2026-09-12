//! Who may do what to a decision.
//!
//! One table rather than a check at each call site, because the interesting
//! rule is the shape of the whole thing: bots raise and withdraw, the owner
//! rules, and nothing in between. A lead bot is told about every decision in
//! its project and can add context — but it cannot answer one, and that is the
//! whole point. The failure this replaces was a lead relaying a ruling it did
//! not have the authority to give.

use bus::{Decision, DecisionState};

use crate::db::Actor;

use super::{forbidden, DecisionError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Comment,
    Withdraw,
    /// Replace the tags on a decision.
    Retag,
    /// Answer, hold, resume, publish, reopen, confirm, edit, delete.
    Rule,
}

/// Check an actor against a decision, or say why not in words the caller can
/// pass straight back to a bot.
pub fn may(actor: &Actor<'_>, action: Action, decision: &Decision) -> Result<(), DecisionError> {
    if actor.is_owner() {
        return Ok(());
    }
    let Some(bot_id) = actor.bot_id() else {
        return Err(DecisionError::Forbidden("unknown caller".to_string()));
    };
    if actor.project_id() != Some(decision.project_id.as_str()) {
        return Err(DecisionError::Forbidden(
            "that decision belongs to another project".to_string(),
        ));
    }
    match action {
        // Any bot in the project may add context. This is how a lead offers a
        // counter-recommendation for the owner to weigh.
        Action::Comment => Ok(()),
        Action::Withdraw => mine_and_unsettled(bot_id, decision, "withdraw"),
        // Filing is how a settled ruling is found again, so retagging someone
        // else's — or anything already published — is the registry quietly
        // losing a record rather than a tidy-up.
        Action::Retag => mine_and_unsettled(bot_id, decision, "retag"),
        Action::Rule => Err(DecisionError::Forbidden(
            "only the owner rules on a decision. Add what you know with comment_decision; \
             the owner will see it in the thread."
                .to_string(),
        )),
    }
}

/// A bot may change its own decision, and only until the owner has ruled.
fn mine_and_unsettled(bot_id: &str, decision: &Decision, verb: &str) -> Result<(), DecisionError> {
    let mine = decision.raised_by_bot_id == bot_id
        || decision.on_behalf_of_bot_id.as_deref() == Some(bot_id);
    if !mine {
        return Err(DecisionError::Forbidden(format!(
            "decision {} is not yours to {verb} — only {} or the owner can",
            decision.id, decision.raised_by_bot_id
        )));
    }
    if !decision.state.is_before_publish() {
        return Err(DecisionError::Conflict(format!(
            "decision {} is already {} — a settled ruling stands until the owner reopens it",
            decision.id,
            decision.state.as_str()
        )));
    }
    Ok(())
}

/// The owner-only gate, for operations that take no decision to check against
/// (creating a tag merge, setting a project's lead).
pub fn require_owner(actor: &Actor<'_>, what: &str) -> anyhow::Result<()> {
    if actor.is_owner() {
        return Ok(());
    }
    Err(forbidden(format!("only the owner can {what}")))
}

/// True when a settled decision is still a bot's relay rather than the owner's
/// own words.
pub fn is_relayed(decision: &Decision) -> bool {
    decision
        .ruling
        .as_ref()
        .is_some_and(|r| r.answered_by.starts_with("owner-via-bot:"))
        && decision.state == DecisionState::Settled
}
