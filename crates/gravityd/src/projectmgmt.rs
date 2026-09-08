//! Project lifecycle: creation, renaming, and archival deletion.
//!
//! A project owns a directory on disk and every bot inside it, so the two
//! mutations here are careful about different things. A rename must not touch
//! the filesystem — `dir_name` is frozen exactly so a running bot's
//! `workspace_path` stays valid — and a delete must go through
//! [`botmgmt::archive_bot`] for each bot rather than dropping rows, or it would
//! leave live runtimes holding valid credentials for a project that is gone.

use std::sync::Arc;

use anyhow::bail;
use bus::Project;

use crate::app::AppState;
use crate::botmgmt;
use crate::db::{Actor, Db};
use crate::events::Push;
use crate::paths;

/// A directory name not yet used by any project, archived ones included.
///
/// Sanitizing is lossy — `"Acme Co"` and `"Acme-Co"` both yield `acme-co` — so
/// two differently named projects would otherwise share one directory. The
/// counter mirrors `botmgmt::unique_dir_name`.
fn unique_dir_name(app: &Arc<AppState>, name: &str) -> anyhow::Result<String> {
    let base = bus::names::dir_name(name);
    if !app.db.project_dir_name_taken(&base)? {
        return Ok(base);
    }
    for n in 2..1000 {
        let candidate = format!("{base}-{n}");
        if !app.db.project_dir_name_taken(&candidate)? {
            return Ok(candidate);
        }
    }
    bail!("cannot allocate a directory for project '{name}'")
}

/// Create the shared artifacts directory for projects provisioned before it
/// existed. Runs at boot; a no-op once every live project has one.
pub fn ensure_artifacts_dirs(app: &Arc<AppState>) -> anyhow::Result<()> {
    for project in app.db.list_projects()? {
        if project.deleted_at.is_some() {
            continue;
        }
        let dir = paths::artifacts_dir(&app.cfg, &project.dir_name);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(project_id = %project.id, error = %e, "artifacts dir creation failed");
        }
    }
    Ok(())
}

/// Create a project and lay out its directory.
pub fn create_project(app: &Arc<AppState>, raw_name: &str) -> anyhow::Result<Project> {
    let name = bus::names::validate_project(raw_name).map_err(|e| anyhow::anyhow!("{e}"))?;
    if app.db.project_name_taken(&name, "")? {
        bail!("a project named '{name}' already exists");
    }
    let dir_name = unique_dir_name(app, &name)?;
    let project = app.db.create_project(&name, &dir_name)?;
    paths::provision_project(&app.cfg, &project.id, &project.dir_name, &project.name)?;
    app.events.push(Push::ProjectUpdated {
        project: project.clone(),
    });
    Ok(project)
}

/// Rename a project, leaving its directory and every workspace inside it alone.
///
/// The name still reaches disk in two descriptive places — the project manifest
/// and each bot's `bot.json` — so both are refreshed here rather than left to
/// disagree with the database.
pub fn rename_project(
    app: &Arc<AppState>,
    project: &Project,
    raw_name: &str,
) -> anyhow::Result<Project> {
    let name = bus::names::validate_project(raw_name).map_err(|e| anyhow::anyhow!("{e}"))?;
    if name == project.name {
        return Ok(project.clone());
    }
    if app.db.project_name_taken(&name, &project.id)? {
        bail!("a project named '{name}' already exists");
    }
    app.db.rename_project(&project.id, &name)?;

    paths::write_project_manifest(&app.cfg, &project.id, &project.dir_name, &name)?;
    for bot in app.db.list_bots(Some(&project.id))? {
        let root = paths::bot_dir(&app.cfg, &project.dir_name, &bot.dir_name);
        if let Err(e) = paths::set_bot_project_name(&root, &name) {
            tracing::warn!(bot_id = %bot.id, error = %e, "bot.json project name not refreshed");
        }
    }

    let renamed = app
        .db
        .get_project(&project.id)?
        .ok_or_else(|| anyhow::anyhow!("project vanished during rename"))?;
    app.events.push(Push::ProjectUpdated {
        project: renamed.clone(),
    });
    tracing::info!(project_id = %renamed.id, from = %project.name, to = %name, "project renamed");
    Ok(renamed)
}

/// Archive a project and every bot in it.
///
/// Returns how many bots were archived. Each goes through the normal bot
/// archival path — stopped, credential revoked, open tasks released — because a
/// project deletion that skipped it would leave orphaned runtimes behind.
pub fn archive_project(
    app: &Arc<AppState>,
    project: &Project,
    actor: &Actor<'_>,
) -> anyhow::Result<usize> {
    let bots = app.db.list_bots(Some(&project.id))?;
    for bot in &bots {
        botmgmt::archive_bot(app, bot, actor, Some("its project was deleted"))?;
    }
    app.db.archive_project(&project.id, &actor.as_stored())?;

    let archived = app
        .db
        .get_project(&project.id)?
        .ok_or_else(|| anyhow::anyhow!("project vanished during delete"))?;
    app.events.push(Push::ProjectUpdated { project: archived });
    app.events.push(Push::Notify {
        level: "info".to_string(),
        title: "Project deleted".to_string(),
        body: match bots.len() {
            0 => format!("{} was deleted.", Db::display_project_name(project)),
            1 => format!(
                "{} was deleted along with its bot.",
                Db::display_project_name(project)
            ),
            n => format!(
                "{} was deleted along with its {n} bots.",
                Db::display_project_name(project)
            ),
        },
    });
    tracing::info!(
        project_id = %project.id, name = %project.name, bots = bots.len(),
        by = %actor.as_stored(), "project archived"
    );
    Ok(bots.len())
}
