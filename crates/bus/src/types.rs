//! Shared entity vocabulary. Split into `enums` (states and triggers) and
//! `entities` (records); both are re-exported here.

use chrono::{DateTime, Utc};

mod decision_enums;
mod decisions;
mod entities;
mod enums;
mod peers;
mod revisions;

pub use decision_enums::*;
pub use decisions::*;
pub use entities::*;
pub use enums::*;
pub use peers::*;
pub use revisions::*;

pub type Id = String;

pub fn new_id() -> Id {
    uuid::Uuid::new_v4().to_string()
}

pub fn now() -> DateTime<Utc> {
    Utc::now()
}
