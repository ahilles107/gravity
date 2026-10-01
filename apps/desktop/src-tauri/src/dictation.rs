//! On-device dictation for the chat composer. The operating system's own
//! speech recognizer turns the microphone into text, so audio never goes to
//! Gravity or a third party: Apple's Speech framework on macOS, set to
//! recognize on the device whenever the Mac supports it.
//!
//! The composer starts and stops a session; partial and final transcripts
//! arrive as `dictation` events.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[cfg(target_os = "macos")]
mod macos;

/// The event the composer listens to.
const EVENT: &str = "dictation";

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum DictationEvent {
    /// The transcript so far; it may still change.
    Partial { text: String },
    /// The settled transcript; the session has ended.
    Final { text: String },
    /// The session ended without a transcript, or failed.
    Ended {
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

pub(crate) fn emit(app: &AppHandle, event: DictationEvent) {
    let _ = app.emit(EVENT, event);
}

/// Whether this platform can dictate.
#[tauri::command]
pub fn dictation_available() -> bool {
    cfg!(target_os = "macos")
}

/// Starts listening, asking for speech and microphone permission the first
/// time. Async so the permission prompt never waits on the main thread.
#[tauri::command]
pub async fn start_dictation(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        tauri::async_runtime::spawn_blocking(move || macos::start(app))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("Dictation is not available on this platform yet.".to_string())
    }
}

/// Stops listening; the final transcript follows as an event.
#[tauri::command]
pub fn stop_dictation() {
    #[cfg(target_os = "macos")]
    macos::stop();
}
