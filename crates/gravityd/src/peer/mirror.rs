//! Keeping linked projects mirrored. Any change to a bot in a linked project
//! (created, renamed, edited, runtime switched, archived) sends the project's
//! whole roster to the peer, which reconciles its stand-ins against it. A
//! whole roster rather than a diff, so a lost event is repaired by the next
//! one, and every link-up resends them all.

use std::collections::HashSet;
use std::sync::Arc;

use bus::ProjectLink;
use serde_json::{json, Value};
use tokio::sync::broadcast::error::RecvError;

use crate::app::AppState;
use crate::events::Push;

use super::links::{self, ProjectSide};
use super::roster;

/// Watches the daemon's own pushes for changes a peer should mirror.
pub fn spawn(app: Arc<AppState>) {
    let mut pushes = app.events.subscribe_push();
    tokio::spawn(async move {
        loop {
            match pushes.recv().await {
                Ok(Push::BotUpdated { bot }) if !bot.is_linked() => {
                    send_project(&app, &bot.project_id);
                }
                Ok(Push::ProjectUpdated { project }) if project.deleted_at.is_some() => {
                    // An archived project has no team left to link.
                    let app = app.clone();
                    tokio::spawn(async move {
                        for link in app.db.project_links(&project.id).unwrap_or_default() {
                            let _ = links::unlink(&app, &project.id, &link.peer_id).await;
                        }
                    });
                }
                Ok(Push::ProjectUpdated { project }) => send_project(&app, &project.id),
                Ok(_) => {}
                Err(RecvError::Lagged(_)) => {
                    for peer in app.db.list_peers().unwrap_or_default() {
                        sync_rosters(&app, &peer.id);
                    }
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}

fn send_project(app: &AppState, project_id: &str) {
    for link in app.db.project_links(project_id).unwrap_or_default() {
        send_roster(app, &link);
    }
}

/// Sends a linked project's roster to its peer, which may now deliver to
/// every bot on it.
fn send_roster(app: &AppState, link: &ProjectLink) {
    if !app.peers.is_online(&link.peer_id) {
        return;
    }
    let sent = (|| -> anyhow::Result<()> {
        let project = app
            .db
            .get_project(&link.project_id)?
            .ok_or_else(|| anyhow::anyhow!("project missing"))?;
        let side = links::side(app, &project)?;
        roster::expose(app, &link.peer_id, &side.bots)?;
        app.peers.notify(
            &link.peer_id,
            json!({ "type": "project_roster", "project": side }),
        );
        Ok(())
    })();
    if let Err(e) = sent {
        tracing::warn!(project_id = %link.project_id, error = %e, "roster not sent");
    }
}

fn sync_rosters(app: &AppState, peer_id: &str) {
    for link in app.db.links_through(peer_id).unwrap_or_default() {
        send_roster(app, &link);
    }
}

/// On link-up: say which links this side holds, so one unlinked while the
/// link was down is dropped on the other side too, then resend every roster.
pub(super) fn sync(app: &Arc<AppState>, peer_id: &str) {
    let held: Vec<Value> = app
        .db
        .links_through(peer_id)
        .unwrap_or_default()
        .iter()
        .map(|l| json!({ "project_id": l.project_id, "remote_project_id": l.remote_project_id }))
        .collect();
    app.peers
        .notify(peer_id, json!({ "type": "project_links", "links": held }));
    sync_rosters(app, peer_id);
}

/// The peer's roster for a project linked here.
pub(super) fn receive_roster(app: &Arc<AppState>, peer_id: &str, frame: &Value) {
    let received = (|| -> anyhow::Result<()> {
        let theirs: ProjectSide = serde_json::from_value(frame["project"].clone())?;
        // A roster can overtake the reply that records a new link; the
        // reply carries the same bots.
        let Some(link) = app.db.project_link_by_remote(peer_id, &theirs.project_id)? else {
            return Ok(());
        };
        let peer = app
            .db
            .get_peer(peer_id)?
            .ok_or_else(|| anyhow::anyhow!("peer missing"))?;
        roster::reconcile(app, &peer, &link.project_id, &theirs.bots)?;
        if app
            .db
            .rename_remote_project(&link.project_id, peer_id, &theirs.project_name)?
        {
            links::push_project(app, &link.project_id);
        }
        Ok(())
    })();
    if let Err(e) = received {
        tracing::warn!(peer_id, error = %e, "linked project roster not applied");
    }
}

/// The links the peer held when its link came up. One recorded here before
/// that, and missing there, was unlinked while the two could not talk.
pub(super) fn receive_links(app: &Arc<AppState>, peer_id: &str, frame: &Value) {
    let Some(since) = app.peers.online_since(peer_id) else {
        return;
    };
    let held: HashSet<(String, String)> = frame["links"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| {
            Some((
                l["remote_project_id"].as_str()?.to_string(),
                l["project_id"].as_str()?.to_string(),
            ))
        })
        .collect();
    for link in app.db.links_through(peer_id).unwrap_or_default() {
        let key = (link.project_id.clone(), link.remote_project_id.clone());
        if link.linked_at < since && !held.contains(&key) {
            tracing::info!(peer_id, project_id = %link.project_id,
                "peer unlinked this project while offline");
            if let Err(e) = links::drop_link(app, &link.project_id, peer_id) {
                tracing::warn!(peer_id, error = %e, "stale link not dropped");
            }
        }
    }
}
