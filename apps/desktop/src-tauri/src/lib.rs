mod daemon;
mod shortcut;
mod updater;

use std::ffi::OsStr;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Reads the daemon client token from `~/.gravity/secrets/client.token`.
#[tauri::command]
fn read_client_token() -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|err| format!("HOME is not set: {err}"))?;
    let path = PathBuf::from(home).join(".gravity/secrets/client.token");
    let token = std::fs::read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Ok(token.trim().to_string())
}

/// Puts the unread count on the dock icon; a count of zero removes the badge.
#[tauri::command]
fn set_dock_badge(app: AppHandle, count: u32) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "the main window is gone".to_string())?;
    let badge = if count == 0 {
        None
    } else {
        Some(i64::from(count))
    };
    window
        .set_badge_count(badge)
        .map_err(|err| format!("failed to set the dock badge: {err}"))
}

fn parse_external_url(value: &str) -> Result<tauri::Url, String> {
    let url = tauri::Url::parse(value).map_err(|err| format!("invalid URL: {err}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("unsupported URL scheme: {}", url.scheme()));
    }
    Ok(url)
}

/// Opens an HTTP(S) URL in the user's default browser.
///
/// Async so the wait for the launcher runs off the main thread: a cold browser
/// start would otherwise freeze the window for as long as it takes.
#[tauri::command]
async fn open_external_url(url: String) -> Result<(), String> {
    let url = parse_external_url(&url)?;
    open_url(url.as_str()).map_err(|err| format!("failed to open {url}: {err}"))
}

/// Runs a launcher and fails when it cannot be started or exits non-zero.
fn launch(program: &str, args: &[&OsStr]) -> Result<(), String> {
    let status = std::process::Command::new(program)
        .args(args)
        .status()
        .map_err(|err| format!("could not run {program}: {err}"))?;
    if status.success() {
        return Ok(());
    }
    Err(format!("{program} exited with {status}"))
}

#[cfg(target_os = "macos")]
fn open_url(url: &str) -> Result<(), String> {
    launch("open", &[url.as_ref()])
}

/// `explorer` applies its own argument parsing and mangles URLs containing
/// commas, so hand the URL to `start` instead. Its first argument is taken as
/// a window title, hence the empty string.
#[cfg(target_os = "windows")]
fn open_url(url: &str) -> Result<(), String> {
    launch(
        "cmd",
        &["/c".as_ref(), "start".as_ref(), "".as_ref(), url.as_ref()],
    )
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn open_url(url: &str) -> Result<(), String> {
    launch("xdg-open", &[url.as_ref()])
}

/// Shows a project's directory in the OS file manager.
///
/// Takes the immutable `dir_name` rather than a path so the webview cannot ask
/// the shell to open somewhere outside `~/.gravity/projects/`.
#[tauri::command]
fn reveal_project(dir_name: String) -> Result<(), String> {
    if dir_name.is_empty() || dir_name.contains('/') || dir_name.contains('\\') {
        return Err(format!("not a project directory name: {dir_name}"));
    }
    let home = std::env::var("HOME").map_err(|err| format!("HOME is not set: {err}"))?;
    let dir = PathBuf::from(home)
        .join(".gravity/projects")
        .join(&dir_name);
    if !dir.is_dir() {
        return Err(format!("no such project directory: {}", dir.display()));
    }
    reveal(&dir).map_err(|err| format!("failed to reveal {}: {err}", dir.display()))
}

fn is_bot_workspace(path: &std::path::Path) -> bool {
    let Some(bot_dir) = path.parent() else {
        return false;
    };
    let Some(bots_dir) = bot_dir.parent() else {
        return false;
    };
    let Some(project_dir) = bots_dir.parent() else {
        return false;
    };
    let Some(projects_dir) = project_dir.parent() else {
        return false;
    };
    path.file_name().and_then(|name| name.to_str()) == Some("workspace")
        && bots_dir.file_name().and_then(|name| name.to_str()) == Some("bots")
        && projects_dir.file_name().and_then(|name| name.to_str()) == Some("projects")
}

/// Shows a bot's workspace in the OS file manager.
///
/// Bot workspaces may live under the installed daemon or a workspace-private
/// development daemon, so the client supplies the path. Its fixed
/// `projects/<project>/bots/<bot>/workspace` shape is validated before use.
#[tauri::command]
fn reveal_bot_workspace(workspace_path: String) -> Result<(), String> {
    let requested = PathBuf::from(&workspace_path);
    if !requested.is_absolute() || !is_bot_workspace(&requested) {
        return Err(format!("not a bot workspace: {}", requested.display()));
    }
    let dir = requested
        .canonicalize()
        .map_err(|err| format!("no such bot workspace {}: {err}", requested.display()))?;
    if !dir.is_dir() || !is_bot_workspace(&dir) {
        return Err(format!("not a bot workspace: {}", dir.display()));
    }
    reveal(&dir).map_err(|err| format!("failed to reveal {}: {err}", dir.display()))
}

#[cfg(target_os = "macos")]
fn reveal(dir: &std::path::Path) -> Result<(), String> {
    launch("open", &["-R".as_ref(), dir.as_os_str()])
}

#[cfg(target_os = "windows")]
fn reveal(dir: &std::path::Path) -> Result<(), String> {
    launch("explorer", &[dir.as_os_str()])
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn reveal(dir: &std::path::Path) -> Result<(), String> {
    launch("xdg-open", &[dir.as_os_str()])
}

/// A Dock click on a window hidden by the global shortcut brings it back.
#[cfg(target_os = "macos")]
fn handle_run_event(app: &AppHandle, event: tauri::RunEvent) {
    if let tauri::RunEvent::Reopen {
        has_visible_windows: false,
        ..
    } = event
    {
        shortcut::show_main_window(app);
    }
}

#[cfg(not(target_os = "macos"))]
fn handle_run_event(_app: &AppHandle, _event: tauri::RunEvent) {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .manage(shortcut::ToggleShortcut::default())
        .invoke_handler(tauri::generate_handler![
            read_client_token,
            set_dock_badge,
            open_external_url,
            reveal_project,
            reveal_bot_workspace,
            shortcut::set_toggle_window_shortcut,
            daemon::daemon_health,
            daemon::install_local_daemon,
            daemon::local_daemon_is_managed,
            daemon::restart_local_daemon,
            daemon::local_daemon_port,
            daemon::daemon_log_tail,
            updater::check_for_update,
            updater::install_update,
            updater::relaunch_app
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(handle_run_event);
}

#[cfg(test)]
mod tests {
    use super::{is_bot_workspace, parse_external_url};
    use std::path::Path;

    #[test]
    fn recognizes_bot_workspace_paths() {
        assert!(is_bot_workspace(Path::new(
            "/tmp/gravity/projects/acme/bots/alice/workspace"
        )));
        assert!(!is_bot_workspace(Path::new(
            "/tmp/gravity/projects/acme/workspace"
        )));
        assert!(!is_bot_workspace(Path::new(
            "/tmp/gravity/projects/acme/bots/alice"
        )));
    }

    #[test]
    fn accepts_only_web_urls_for_external_navigation() {
        assert!(parse_external_url("https://example.com/docs").is_ok());
        assert!(parse_external_url("http://localhost:5173").is_ok());
        assert!(parse_external_url("javascript:alert(1)").is_err());
        assert!(parse_external_url("not a url").is_err());
    }
}
