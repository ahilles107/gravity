//! Shared validation for routine inputs from the control plane and MCP.

use bus::MAX_MESSAGE_BYTES;
use serde_json::Value;

use crate::db::RoutineLimits;

pub const MAX_ROUTINE_NAME_BYTES: usize = 200;
pub const MAX_ROUTINE_DURATION_SECONDS: i64 = 6 * 60 * 60;
pub const MAX_ROUTINE_ATTEMPTS: i64 = 5;
pub const MAX_SIGNAL_PAYLOAD_BYTES: usize = 16 * 1024;
pub const DEFAULT_RUN_HISTORY_LIMIT: i64 = 50;
pub const MAX_RUN_HISTORY_LIMIT: i64 = 200;

pub fn checked_routine_fields(name: Option<&str>, prompt: Option<&str>) -> anyhow::Result<()> {
    if let Some(name) = name {
        if name.len() > MAX_ROUTINE_NAME_BYTES {
            anyhow::bail!(
                "'name' is {} bytes; the limit is {MAX_ROUTINE_NAME_BYTES}",
                name.len()
            );
        }
    }
    if let Some(prompt) = prompt {
        if prompt.len() > MAX_MESSAGE_BYTES {
            anyhow::bail!(
                "'prompt' is {} bytes; the limit is {MAX_MESSAGE_BYTES}",
                prompt.len()
            );
        }
    }
    Ok(())
}

fn optional_i64(args: &Value, key: &str) -> anyhow::Result<Option<i64>> {
    match args.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("'{key}' must be an integer")),
    }
}

pub fn checked_limits(args: &Value) -> anyhow::Result<RoutineLimits> {
    let max_duration_seconds = optional_i64(args, "max_duration_seconds")?;
    if let Some(duration) = max_duration_seconds {
        if !(1..=MAX_ROUTINE_DURATION_SECONDS).contains(&duration) {
            anyhow::bail!(
                "max_duration_seconds must be between 1 and {MAX_ROUTINE_DURATION_SECONDS}"
            );
        }
    }
    let max_attempts = optional_i64(args, "max_attempts")?;
    if let Some(attempts) = max_attempts {
        if !(1..=MAX_ROUTINE_ATTEMPTS).contains(&attempts) {
            anyhow::bail!("max_attempts must be between 1 and {MAX_ROUTINE_ATTEMPTS}");
        }
    }
    Ok(RoutineLimits {
        max_duration_seconds,
        max_attempts,
    })
}

pub fn checked_signal_payload(payload: &Value) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(payload)?.len();
    if bytes > MAX_SIGNAL_PAYLOAD_BYTES {
        anyhow::bail!("payload is {bytes} bytes; the limit is {MAX_SIGNAL_PAYLOAD_BYTES}");
    }
    Ok(())
}

pub fn run_history_limit(req: &Value) -> anyhow::Result<i64> {
    let Some(value) = req.get("limit") else {
        return Ok(DEFAULT_RUN_HISTORY_LIMIT);
    };
    let limit = value
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("'limit' must be an integer"))?;
    if !(1..=MAX_RUN_HISTORY_LIMIT).contains(&limit) {
        anyhow::bail!("limit must be between 1 and {MAX_RUN_HISTORY_LIMIT}");
    }
    Ok(limit)
}
