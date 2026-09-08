//! Shared entity vocabulary. Split into `enums` (states and triggers) and
//! `entities` (records); both are re-exported here.

use chrono::{DateTime, Utc};

mod entities;
mod enums;

pub use entities::*;
pub use enums::*;

pub type Id = String;

pub fn new_id() -> Id {
    uuid::Uuid::new_v4().to_string()
}

pub fn now() -> DateTime<Utc> {
    Utc::now()
}
