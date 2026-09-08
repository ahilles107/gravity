//! The global shortcut that shows or hides the main window from any app.

use std::sync::Mutex;

use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// The accelerator currently registered with the OS; empty means none.
#[derive(Default)]
pub struct ToggleShortcut(Mutex<String>);

/// Brings the main window forward, restoring it if minimized or hidden.
pub fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    // Toggling off hides the whole app on macOS, so unhide it before ordering
    // the window front.
    #[cfg(target_os = "macos")]
    let _ = app.show();
    if window.is_minimized().unwrap_or(false) {
        let _ = window.unminimize();
    }
    let _ = window.show();
    let _ = window.set_focus();
}

fn is_frontmost(window: &WebviewWindow) -> bool {
    window.is_visible().unwrap_or(false)
        && window.is_focused().unwrap_or(false)
        && !window.is_minimized().unwrap_or(false)
}

/// Hides the main window when it is frontmost, otherwise brings it forward.
///
/// macOS hides the whole app rather than the window, so the app the user came
/// from becomes frontmost again instead of leaving focus on nothing.
fn toggle_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if !is_frontmost(&window) {
        show_main_window(app);
        return;
    }
    #[cfg(target_os = "macos")]
    let _ = app.hide();
    #[cfg(not(target_os = "macos"))]
    let _ = window.hide();
}

/// Replaces the show/hide shortcut; an empty string leaves none registered.
///
/// The accelerator uses the plugin's syntax (`Super+Shift+KeyG`). The new
/// combination is registered before the old one is dropped, so a refusal —
/// typically another app already owns the keys — leaves the previous shortcut
/// live and matching what settings shows.
#[tauri::command]
pub fn set_toggle_window_shortcut(
    app: AppHandle,
    state: State<'_, ToggleShortcut>,
    shortcut: String,
) -> Result<(), String> {
    let mut current = state
        .0
        .lock()
        .map_err(|_| "the shortcut state is poisoned".to_string())?;
    if *current == shortcut {
        return Ok(());
    }
    let shortcuts = app.global_shortcut();
    if !shortcut.is_empty() {
        shortcuts
            .on_shortcut(shortcut.as_str(), |app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    toggle_main_window(app);
                }
            })
            .map_err(|err| format!("failed to register {shortcut}: {err}"))?;
    }
    if !current.is_empty() {
        let _ = shortcuts.unregister(current.as_str());
    }
    *current = shortcut;
    Ok(())
}
