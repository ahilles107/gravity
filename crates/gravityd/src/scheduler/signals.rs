//! Emitting a signal and fanning it out to the routines that subscribe. This
//! is the whole non-time trigger surface: matching is by name, optionally
//! narrowed to one emitter, and always scoped to a project.

use bus::{RunSource, Signal, SignalSource};
use serde_json::Value;

use crate::db::{Db, NewRun, NewSignal};
use crate::events::{Events, Push};

/// Backstop for a ring longer than the `origin_chain` membership check can
/// catch (A emits, B reacts and emits, C reacts back at A's neighbour).
pub const MAX_SIGNAL_HOPS: i64 = 4;

/// Names are matched exactly and read in run history: addresses, not prose.
const MAX_SIGNAL_NAME_BYTES: usize = 100;

pub struct EmitSignal<'a> {
    pub name: &'a str,
    pub source: SignalSource,
    pub project_id: &'a str,
    pub from_bot_id: Option<&'a str>,
    pub payload: Value,
}

pub fn validate_signal_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() || name.len() > MAX_SIGNAL_NAME_BYTES {
        anyhow::bail!("signal name must be 1..={MAX_SIGNAL_NAME_BYTES} bytes");
    }
    let ok = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'));
    if !ok {
        anyhow::bail!(
            "signal name '{name}' may only contain lowercase letters, digits, '.', '-' and '_'"
        );
    }
    Ok(())
}

/// Emit a signal and create an occurrence for every routine that subscribes.
pub fn emit_signal(db: &Db, events: &Events, req: EmitSignal<'_>) -> anyhow::Result<Signal> {
    validate_signal_name(req.name)?;
    let (origin_chain, hop_count) = chain_for(db, req.from_bot_id)?;
    if hop_count > MAX_SIGNAL_HOPS {
        anyhow::bail!(
            "signal '{}' is {hop_count} hops from its origin; the limit is {MAX_SIGNAL_HOPS}",
            req.name
        );
    }
    let signal = db.emit_signal(NewSignal {
        name: req.name,
        source: req.source,
        project_id: req.project_id,
        from_bot_id: req.from_bot_id,
        payload: req.payload,
        origin_chain,
        hop_count,
    })?;
    fan_out(db, events, &signal)?;
    Ok(signal)
}

/// The chain a signal inherits from the work that caused it, inferred from
/// the bot's running signal-triggered run. With none, the bot is the origin.
fn chain_for(db: &Db, from_bot_id: Option<&str>) -> anyhow::Result<(String, i64)> {
    let Some(bot_id) = from_bot_id else {
        return Ok((String::new(), 0));
    };
    let Some(parent) = db.causing_signal_for_bot(bot_id)? else {
        return Ok((bot_id.to_string(), 0));
    };
    let mut chain: Vec<&str> = parent
        .origin_chain
        .split(',')
        .filter(|s| !s.is_empty())
        .collect();
    if !chain.contains(&bot_id) {
        chain.push(bot_id);
    }
    Ok((chain.join(","), parent.hop_count + 1))
}

fn fan_out(db: &Db, events: &Events, signal: &Signal) -> anyhow::Result<()> {
    for routine in db.signal_routines(
        &signal.project_id,
        &signal.name,
        signal.from_bot_id.as_deref(),
    )? {
        if !subscribes(&routine.bot_id, signal) {
            continue;
        }
        let created = db.schedule_routine_run(NewRun {
            routine_id: &routine.id,
            scheduled_for: signal.emitted_at,
            source: RunSource::Signal,
            signal_id: Some(&signal.id),
        })?;
        if let Some(run) = created {
            events.push(Push::RoutineRunUpdate { routine_run: run });
        }
    }
    Ok(())
}

/// Same project, and not already part of the chain that produced it.
fn subscribes(owner_bot_id: &str, signal: &Signal) -> bool {
    !signal.origin_chain.split(',').any(|id| id == owner_bot_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_names_that_are_not_addresses() {
        for bad in ["", "Deploy.Finished", "deploy finished", "deploy/finished"] {
            assert!(
                validate_signal_name(bad).is_err(),
                "{bad} should be rejected"
            );
        }
        assert!(validate_signal_name(&"a".repeat(MAX_SIGNAL_NAME_BYTES + 1)).is_err());
    }

    #[test]
    fn accepts_dotted_lowercase_names() {
        for good in ["deploy.finished", "ci_failed", "pr-opened", "x1"] {
            assert!(
                validate_signal_name(good).is_ok(),
                "{good} should be accepted"
            );
        }
    }
}
