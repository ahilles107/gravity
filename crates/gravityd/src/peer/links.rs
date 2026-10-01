//! Linked projects: a project here and one on a peer working as one team.
//! Every bot on each side stands in on the other, a link is recorded on both
//! daemons, and unlinking from either side archives the stand-ins on both.
//! See "Linked projects" in `docs/peer-bots.md`.

use std::sync::Arc;

use bus::{Peer, Project, RemoteBot};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::app::AppState;
use crate::botmgmt;
use crate::db::Db;
use crate::events::Push;

use super::{refuse, roster};

/// What one side of a link tells the other about its project. The ids are
/// the sender's own.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ProjectSide {
    pub project_id: String,
    pub project_name: String,
    #[serde(default)]
    pub bots: Vec<RemoteBot>,
}

pub(super) fn side(app: &AppState, project: &Project) -> anyhow::Result<ProjectSide> {
    Ok(ProjectSide {
        project_id: project.id.clone(),
        project_name: Db::display_project_name(project),
        bots: roster::roster(app, &project.id)?,
    })
}

fn live_peer(app: &AppState, peer_id: &str) -> anyhow::Result<Peer> {
    app.db
        .get_peer(peer_id)?
        .filter(|p| p.revoked_at.is_none())
        .ok_or_else(|| refuse("not_found", "peer not found or revoked"))
}

fn live_project(app: &AppState, project_id: &str) -> anyhow::Result<Project> {
    app.db
        .get_live_project(project_id)?
        .ok_or_else(|| refuse("not_found", "project not found"))
}

// ---- this side asks ----

/// Links a project here with one on the peer: `remote_project_id` names an
/// existing one, otherwise the peer creates one named `remote_name` or like
/// this project.
pub async fn link(
    app: &Arc<AppState>,
    project_id: &str,
    peer_id: &str,
    remote_project_id: Option<&str>,
    remote_name: Option<&str>,
) -> anyhow::Result<Project> {
    let project = live_project(app, project_id)?;
    let peer = live_peer(app, peer_id)?;
    if !app.peers.is_online(peer_id) {
        return Err(refuse("unavailable", format!("{} is offline", peer.name)));
    }
    if let Some(link) = app.db.project_link(project_id, peer_id)? {
        return Err(refuse(
            "conflict",
            format!(
                "{} is already linked with {} on {}",
                project.name, link.remote_project_name, peer.name
            ),
        ));
    }
    let mut frame = json!({ "type": "link_project", "project": side(app, &project)? });
    if let Some(id) = remote_project_id {
        frame["remote_project_id"] = json!(id);
    }
    if let Some(name) = remote_name {
        frame["remote_name"] = json!(name);
    }
    let result = app.peers.request(peer_id, frame).await?;
    let theirs: ProjectSide = serde_json::from_value(result)?;
    let adopted = (|| -> anyhow::Result<()> {
        let clashing = roster::clashes(app, project_id, peer_id, &theirs.bots)?;
        if !clashing.is_empty() {
            return Err(roster::clash_refusal(&clashing));
        }
        adopt(app, &peer, &project, &theirs)
    })();
    if let Err(e) = adopted {
        // The peer has recorded its half; take it back.
        let undo = json!({ "type": "unlink_project", "project_id": project_id });
        let _ = app.peers.request(peer_id, undo).await;
        return Err(e);
    }
    tracing::info!(peer = %peer.name, project = %project.name,
        remote = %theirs.project_name, "project linked");
    Ok(project)
}

/// Unlinks a project from a peer on both sides. An offline peer hears about
/// it when its link comes back.
pub async fn unlink(
    app: &Arc<AppState>,
    project_id: &str,
    peer_id: &str,
) -> anyhow::Result<Project> {
    let project = app
        .db
        .get_project(project_id)?
        .ok_or_else(|| refuse("not_found", "project not found"))?;
    if !drop_link(app, project_id, peer_id)? {
        return Err(refuse(
            "not_linked",
            "this project is not linked through that peer",
        ));
    }
    tell_unlinked(app, peer_id, project_id).await;
    Ok(project)
}

/// Unlinks every project linked through a peer being revoked, telling the
/// peer while its link is still up.
pub async fn unlink_all(app: &Arc<AppState>, peer_id: &str) {
    let links = app.db.links_through(peer_id).unwrap_or_default();
    for link in links {
        if let Err(e) = drop_link(app, &link.project_id, peer_id) {
            tracing::warn!(peer_id, project_id = %link.project_id, error = %e, "unlink failed");
        }
        tell_unlinked(app, peer_id, &link.project_id).await;
    }
}

async fn tell_unlinked(app: &Arc<AppState>, peer_id: &str, project_id: &str) {
    let frame = json!({ "type": "unlink_project", "project_id": project_id });
    if let Err(e) = app.peers.request(peer_id, frame).await {
        tracing::info!(peer_id, project_id, error = %e, "peer not told of the unlink yet");
    }
}

// ---- both sides ----

/// Records the link on this side and stands the peer's bots in.
fn adopt(
    app: &Arc<AppState>,
    peer: &Peer,
    project: &Project,
    theirs: &ProjectSide,
) -> anyhow::Result<()> {
    app.db.create_project_link(
        &project.id,
        &peer.id,
        &theirs.project_id,
        &theirs.project_name,
    )?;
    roster::expose(app, &peer.id, &roster::roster(app, &project.id)?)?;
    roster::reconcile(app, peer, &project.id, &theirs.bots)?;
    links_changed(app, &project.id);
    Ok(())
}

/// Forgets a link on this side and archives the peer's stand-ins in the
/// project. False when there was no such link.
pub(crate) fn drop_link(
    app: &Arc<AppState>,
    project_id: &str,
    peer_id: &str,
) -> anyhow::Result<bool> {
    // Forgotten first, so a roster arriving meanwhile finds no link to apply
    // and a message in flight finds no bot it may reach.
    if !app.db.delete_project_link(project_id, peer_id)? {
        return Ok(false);
    }
    app.db.unexpose_project(project_id, peer_id)?;
    for bot in app.db.stand_ins(project_id, peer_id)? {
        roster::archive_stand_in(app, &bot.id, "project unlinked")?;
    }
    tracing::info!(project_id, peer_id, "project unlinked");
    links_changed(app, project_id);
    Ok(true)
}

/// Tells clients the project's links changed, and its bots which machines
/// their team now spans.
fn links_changed(app: &Arc<AppState>, project_id: &str) {
    push_project(app, project_id);
    for bot in app.db.list_bots(Some(project_id)).unwrap_or_default() {
        if let Err(e) = botmgmt::reprovision(app, &bot) {
            tracing::warn!(bot_id = %bot.id, error = %e, "system prompt not refreshed");
        }
    }
}

pub(super) fn push_project(app: &AppState, project_id: &str) {
    if let Ok(Some(project)) = app.db.get_project(project_id) {
        app.events.push(Push::ProjectUpdated { project });
    }
}

// ---- the peer asks ----

/// The projects a peer could link with. Pairing is the owner's consent for
/// either side to see and propose links.
pub(super) fn serve_list(app: &AppState, peer: &Peer) -> anyhow::Result<Value> {
    let mut projects = Vec::new();
    for project in app.db.list_projects()? {
        if project.deleted_at.is_some() {
            continue;
        }
        let link = app.db.project_link(&project.id, &peer.id)?;
        projects.push(json!({
            "id": project.id,
            "name": Db::display_project_name(&project),
            "bot_count": app.db.count_live_bots(&project.id)?,
            "linked_project_id": link.map(|l| l.remote_project_id),
        }));
    }
    Ok(json!({ "projects": projects }))
}

/// A peer proposes a link. Checked in full before anything is created, so a
/// refusal leaves nothing behind.
pub(super) fn serve_link(app: &Arc<AppState>, peer: &Peer, frame: &Value) -> anyhow::Result<Value> {
    let theirs: ProjectSide = serde_json::from_value(frame["project"].clone())?;
    if let Some(link) = app
        .db
        .project_link_by_remote(&peer.id, &theirs.project_id)?
    {
        return Err(refuse(
            "conflict",
            format!(
                "{} is already linked with a project on {}",
                theirs.project_name,
                app.db
                    .get_project(&link.project_id)?
                    .map_or_else(String::new, |p| p.name)
            ),
        ));
    }
    let project = match frame.get("remote_project_id").and_then(Value::as_str) {
        Some(id) => {
            let project = live_project(app, id)?;
            if app.db.project_link(&project.id, &peer.id)?.is_some() {
                return Err(refuse(
                    "conflict",
                    format!("{} is already linked with another project", project.name),
                ));
            }
            let clashing = roster::clashes(app, &project.id, &peer.id, &theirs.bots)?;
            if !clashing.is_empty() {
                return Err(roster::clash_refusal(&clashing));
            }
            project
        }
        None => {
            let name = frame
                .get("remote_name")
                .and_then(Value::as_str)
                .unwrap_or(&theirs.project_name);
            let name = bus::names::validate_project(name).map_err(anyhow::Error::msg)?;
            if app.db.project_name_taken(&name, "")? {
                return Err(refuse(
                    "conflict",
                    format!("a project named '{name}' already exists here; link that one instead"),
                ));
            }
            crate::projectmgmt::create_project(app, &name)?
        }
    };
    adopt(app, peer, &project, &theirs)?;
    tracing::info!(peer = %peer.name, project = %project.name, remote = %theirs.project_name,
        "peer linked a project; pairing is the owner's consent to link");
    Ok(serde_json::to_value(side(app, &project)?)?)
}

/// The peer unlinked its project from this one, or took back a link it
/// could not complete.
pub(super) fn serve_unlink(
    app: &Arc<AppState>,
    peer: &Peer,
    frame: &Value,
) -> anyhow::Result<Value> {
    let remote = frame
        .get("project_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("'project_id' is required"))?;
    if let Some(link) = app.db.project_link_by_remote(&peer.id, remote)? {
        drop_link(app, &link.project_id, &peer.id)?;
        tracing::info!(peer = %peer.name, project_id = %link.project_id, "peer unlinked a project");
    }
    Ok(json!({}))
}
