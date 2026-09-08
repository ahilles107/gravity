//! Shared types, schema, and envelope format for Gravity.
//!
//! This crate is the single source of truth for entity shapes used by the
//! daemon and (via `docs/protocol.md`) the desktop client.

pub mod avatar;
pub mod envelope;
pub mod names;
pub mod schema;
pub mod types;

pub use types::*;
