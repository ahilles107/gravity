//! Self-update commands backed by the Tauri updater plugin. Release builds
//! supply an endpoint and public verification key through a config override.
//! Source builds have no configured update channel.

use serde::Serialize;
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize)]
pub struct UpdateInfo {
    pub version: String,
}

/// Asks the update endpoint whether a newer build exists. `None` means this
/// build is current. Errors (offline, endpoint down) are returned rather than
/// swallowed so the caller can decide to stay quiet.
#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|err| err.to_string())?;
    let update = updater.check().await.map_err(|err| err.to_string())?;
    Ok(update.map(|update| UpdateInfo {
        version: update.version,
    }))
}

/// Downloads and installs the pending update, refreshes an existing managed
/// local daemon from the newly installed sidecar when requested, then
/// relaunches the app. The check runs again here rather than reusing an earlier
/// result so a stale prompt can never install an older artifact.
///
/// Returns `Some(message)` when the app updated but the daemon refresh did
/// not: the app is left running so the caller can surface the failure, and the
/// relaunch moves to `relaunch_app`. A full success never returns at all.
#[tauri::command]
pub async fn install_update(
    app: tauri::AppHandle,
    update_local_daemon: bool,
) -> Result<Option<String>, String> {
    let updater = app.updater().map_err(|err| err.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "no update available".to_string())?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|err| err.to_string())?;
    if update_local_daemon {
        if let Err(err) = crate::daemon::update_local_daemon_if_installed() {
            return Ok(Some(err));
        }
    }
    app.restart();
}

/// Relaunches the app into an already-installed update, split out so a failed
/// daemon refresh can be reported before the window goes away.
#[tauri::command]
pub fn relaunch_app(app: tauri::AppHandle) {
    app.restart();
}
