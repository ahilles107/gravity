//! Undoing an identity change.
//!
//! Nothing asks the user before a bot renames itself or rewrites its own
//! instructions, so the audit trail is only half the answer — this is the other
//! half. Reverting is an ordinary edit applied in the user's name, so it is
//! itself recorded and can in turn be undone.

use std::sync::Arc;

use anyhow::bail;
use bus::{Bot, RevisionField};

use crate::app::AppState;
use crate::db::Actor;

use super::{apply_identity_edit, IdentityEdit};

/// Restore the value an earlier revision replaced.
pub fn revert_revision(app: &Arc<AppState>, revision_id: &str) -> anyhow::Result<Bot> {
    let rev = app
        .db
        .get_bot_revision(revision_id)?
        .ok_or_else(|| anyhow::anyhow!("revision not found"))?;
    if !rev.field.is_revertible() {
        bail!(
            "'{}' is a lifecycle marker, not a field change",
            rev.field.as_str()
        );
    }
    let bot = app
        .db
        .get_live_bot(&rev.bot_id)?
        .ok_or_else(|| anyhow::anyhow!("bot not found or deleted"))?;
    let old = rev.old_value.as_str();
    let edit = match rev.field {
        RevisionField::Name => IdentityEdit {
            name: Some(old),
            ..Default::default()
        },
        RevisionField::Description => IdentityEdit {
            description: Some(old),
            ..Default::default()
        },
        RevisionField::Instructions => IdentityEdit {
            instructions: Some(old),
            ..Default::default()
        },
        RevisionField::Avatar => IdentityEdit {
            avatar: Some(old),
            ..Default::default()
        },
        RevisionField::Created | RevisionField::Deleted => unreachable!("guarded above"),
    };
    apply_identity_edit(app, &bot, &edit, &Actor::User)
}
