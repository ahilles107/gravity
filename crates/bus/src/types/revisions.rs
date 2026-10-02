//! The bot identity audit trail.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Id;

/// One field-level change to a bot's identity: the audit trail that makes
/// unsupervised self-management reversible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotRevision {
    pub id: Id,
    pub bot_id: Id,
    /// `user` or `bot:<id>`.
    pub changed_by: String,
    pub field: RevisionField,
    pub old_value: String,
    pub new_value: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionField {
    Name,
    Avatar,
    Description,
    Instructions,
    /// Lifecycle markers rather than field edits; `new_value` carries the
    /// reason, so a bot's whole life is one ordered list.
    Created,
    Deleted,
}

impl RevisionField {
    pub fn as_str(self) -> &'static str {
        match self {
            RevisionField::Name => "name",
            RevisionField::Avatar => "avatar",
            RevisionField::Description => "description",
            RevisionField::Instructions => "instructions",
            RevisionField::Created => "created",
            RevisionField::Deleted => "deleted",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "name" => Some(RevisionField::Name),
            "avatar" => Some(RevisionField::Avatar),
            "description" => Some(RevisionField::Description),
            "instructions" => Some(RevisionField::Instructions),
            "created" => Some(RevisionField::Created),
            "deleted" => Some(RevisionField::Deleted),
            _ => None,
        }
    }

    /// Whether reverting this entry means writing `old_value` back. Lifecycle
    /// markers are history, not state, so they are not revertible.
    pub fn is_revertible(self) -> bool {
        !matches!(self, RevisionField::Created | RevisionField::Deleted)
    }
}
