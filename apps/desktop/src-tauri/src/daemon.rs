//! Setup-wizard commands: probe a daemon's health endpoint and install the
//! bundled sidecar daemon as a launchd user agent on this machine.

use std::path::{Path, PathBuf};
use std::time::Duration;

use semver::Version;
use serde::Serialize;

#[derive(Serialize)]
pub struct DaemonHealth {
    pub status: String,
    pub version: String,
}

/// Probes `http://<host>:<port>/health`; `None` means nothing answered.
/// Runs in Rust because the webview origin cannot make cross-origin requests
/// to the daemon.
#[tauri::command]
pub fn daemon_health(host: String, port: u16) -> Option<DaemonHealth> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(3))
        .build();
    let body: serde_json::Value = agent
        .get(&format!("http://{host}:{port}/health"))
        .call()
        .ok()?
        .into_json()
        .ok()?;
    Some(DaemonHealth {
        status: body.get("status")?.as_str()?.to_string(),
        version: body.get("version")?.as_str()?.to_string(),
    })
}

/// The bundled `gravityd` sidecar, which Tauri places next to the app
/// binary inside `Contents/MacOS`.
fn sidecar_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|err| format!("cannot locate the app: {err}"))?;
    let path = sidecar_path_for_exe(&exe)?;
    if !path.exists() {
        return Err(format!("bundled daemon not found at {}", path.display()));
    }
    Ok(path)
}

fn sidecar_path_for_exe(exe: &Path) -> Result<PathBuf, String> {
    let dir = exe
        .parent()
        .ok_or_else(|| "the app path has no parent".to_string())?;
    Ok(dir.join("gravityd"))
}

/// Must match `LAUNCHD_LABEL` in `crates/gravityd/src/service.rs`.
const LAUNCHD_LABEL: &str = "in.mikolajczuk.gravityd";

fn user_home() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|err| format!("HOME is not set: {err}"))?;
    Ok(PathBuf::from(home))
}

/// Mirrors `ServicePaths::bin_path` and `plist_path` in `crates/gravityd/src/service.rs`.
fn managed_daemon_is_installed(home: &Path, user_home: &Path) -> bool {
    home.join("bin/gravityd").is_file()
        && user_home
            .join("Library/LaunchAgents")
            .join(format!("{LAUNCHD_LABEL}.plist"))
            .is_file()
}

/// Runs the bundled sidecar's own `service <action>`, the same code path the
/// CLI uses.
fn run_bundled_service(action: &str, failure: &str) -> Result<(), String> {
    let sidecar = sidecar_path()?;
    let out = std::process::Command::new(&sidecar)
        .args(["service", action])
        .output()
        .map_err(|err| format!("failed to run {}: {err}", sidecar.display()))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{failure}: {}", stderr.trim()));
    }
    Ok(())
}

fn install_bundled_local_daemon() -> Result<(), String> {
    run_bundled_service("install", "daemon install failed")
}

/// Whether the daemon answering on `port` is the app-managed launchd agent on
/// this machine, the only one [`restart_local_daemon`] can bounce.
///
/// The port has to match too, not just the install: `scripts/dev.sh` runs a
/// workspace-private daemon as a plain child process on its own port, and
/// offering to bounce a launchd agent that is not the daemon on screen is worse
/// than offering nothing.
#[tauri::command]
pub fn local_daemon_is_managed(port: u16) -> bool {
    let (Ok(home), Ok(user_home)) = (daemon_home(), user_home()) else {
        return false;
    };
    managed_daemon_is_installed(&home, &user_home) && daemon_port_from_home(&home) == port
}

/// Bounces the launchd agent on this machine, which stops every bot session the
/// daemon is running. Refuses when this machine has no app-managed daemon —
/// a remote daemon is not ours to restart.
///
/// Async so the launchctl round-trip runs off the main thread; bootout waits
/// for the daemon to actually exit.
#[tauri::command]
pub async fn restart_local_daemon() -> Result<(), String> {
    if !managed_daemon_is_installed(&daemon_home()?, &user_home()?) {
        return Err("no app-managed daemon is installed on this machine".to_string());
    }
    run_bundled_service("restart", "daemon restart failed")
}

/// Installs and starts the daemon on this machine by running the bundled
/// sidecar's own `service install`, the same code path the CLI uses.
fn reject_daemon_downgrade(current_version: Option<&str>) -> Result<(), String> {
    let Some(current_version) = current_version else {
        return Ok(());
    };
    let bundled = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|err| format!("invalid bundled daemon version: {err}"))?;
    let current = Version::parse(current_version)
        .map_err(|err| format!("refusing to replace daemon with unknown version: {err}"))?;
    if current > bundled {
        return Err(format!(
            "refusing to downgrade daemon from {current} to {bundled}; update Gravity instead"
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn install_local_daemon(current_version: Option<String>) -> Result<(), String> {
    reject_daemon_downgrade(current_version.as_deref())?;
    install_bundled_local_daemon()
}

/// Updates an app-managed launchd daemon, but never creates a local service
/// for a machine that only connects to a remote daemon.
pub(crate) fn update_local_daemon_if_installed() -> Result<(), String> {
    let home = daemon_home()?;
    if !managed_daemon_is_installed(&home, &user_home()?) {
        return Ok(());
    }
    install_bundled_local_daemon()
}

/// Must match `gravityd`'s own config default and `DEFAULT_ENDPOINT` in the
/// client.
const DEFAULT_PORT: u16 = 49777;

/// Root of the daemon's state, mirroring `gravityd`'s own resolution.
fn daemon_home() -> Result<PathBuf, String> {
    if let Some(home) = std::env::var_os("GRAVITY_HOME") {
        return Ok(PathBuf::from(home));
    }
    Ok(user_home()?.join(".gravity"))
}

/// The port the daemon installed on this machine actually serves on.
///
/// A managed daemon publishes its selected port in `gravityd.port`, including
/// any fallback negotiated because the configured port was occupied.
///
/// An install predating the 49777 default still has `port = 7777` in its
/// `gravityd.toml`, and `service install` deliberately keeps an existing config,
/// so the daemon keeps serving on the old port after an upgrade. The wizard
/// has to ask the config rather than assume the current default, or it polls
/// a port nothing will ever answer on.
#[tauri::command]
pub fn local_daemon_port() -> u16 {
    let Ok(home) = daemon_home() else {
        return DEFAULT_PORT;
    };
    daemon_port_from_home(&home)
}

fn daemon_port_from_home(home: &Path) -> u16 {
    if let Ok(text) = std::fs::read_to_string(home.join("gravityd.port")) {
        if let Ok(port) = text.trim().parse::<u16>() {
            if port > 0 {
                return port;
            }
        }
    }
    let Ok(text) = std::fs::read_to_string(home.join("gravityd.toml")) else {
        return DEFAULT_PORT;
    };
    let Ok(parsed) = text.parse::<toml::Table>() else {
        return DEFAULT_PORT;
    };
    parsed
        .get("port")
        .and_then(toml::Value::as_integer)
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port > 0)
        .unwrap_or(DEFAULT_PORT)
}

/// How many distinct trailing log lines to show when an install fails.
///
/// Four covered a bare panic, but the daemon now logs its startup sequence
/// (`gravityd starting`, `listening`, `gravityd stopping`), and a restart loop
/// only reads as one across several lines.
const LOG_TAIL_LINES: usize = 12;

fn tail_of(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines: Vec<&str> = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        // A daemon that cannot bind its port is restarted by launchd every few
        // seconds and logs the same line each time.
        if lines.last() != Some(&line) {
            lines.push(line);
        }
    }
    let tail = lines.split_off(lines.len().saturating_sub(LOG_TAIL_LINES));
    if tail.is_empty() {
        return None;
    }
    Some(tail.join("\n"))
}

/// The daemon's most recent log, so a failed install can say *why* it never
/// came up. The daemon logs to stderr, so `gravityd.err.log` normally holds
/// everything; the stdout log is a fallback for a build that logged there.
/// `None` means there is nothing to show.
#[tauri::command]
pub fn daemon_log_tail() -> Option<String> {
    let logs = daemon_home().ok()?.join("logs");
    tail_of(&logs.join("gravityd.err.log")).or_else(|| tail_of(&logs.join("gravityd.out.log")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn daemon_test_home(name: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!(
            "gravity-desktop-daemon-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("temporary home");
        home
    }

    #[test]
    fn discovers_the_renamed_sidecar_next_to_the_app_binary() {
        let exe = Path::new("/Applications/Gravity.app/Contents/MacOS/Gravity");
        assert_eq!(
            sidecar_path_for_exe(exe).expect("sidecar path"),
            PathBuf::from("/Applications/Gravity.app/Contents/MacOS/gravityd")
        );
    }

    #[test]
    fn recognizes_only_complete_managed_daemon_installs() {
        let root = std::env::temp_dir().join(format!(
            "gravity-desktop-daemon-test-{}",
            std::process::id()
        ));
        // A previous run that panicked mid-test leaves the plist behind, and
        // pids are reused.
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("gravity");
        let user_home = root.join("user");
        let bin = home.join("bin/gravityd");
        let plist = user_home
            .join("Library/LaunchAgents")
            .join(format!("{LAUNCHD_LABEL}.plist"));
        std::fs::create_dir_all(bin.parent().expect("binary parent")).expect("binary directory");
        std::fs::write(&bin, b"daemon").expect("daemon binary");
        assert!(!managed_daemon_is_installed(&home, &user_home));

        std::fs::create_dir_all(plist.parent().expect("plist parent")).expect("plist directory");
        std::fs::write(&plist, b"plist").expect("launchd plist");
        assert!(managed_daemon_is_installed(&home, &user_home));

        std::fs::remove_dir_all(root).expect("test cleanup");
    }

    #[test]
    fn rejects_known_daemon_downgrades() {
        let bundled = Version::parse(env!("CARGO_PKG_VERSION")).expect("bundled version");
        let newer = Version::new(bundled.major + 1, 0, 0).to_string();

        assert!(reject_daemon_downgrade(None).is_ok());
        assert!(reject_daemon_downgrade(Some(&bundled.to_string())).is_ok());
        assert!(reject_daemon_downgrade(Some(&newer)).is_err());
        assert!(reject_daemon_downgrade(Some("dev")).is_err());
    }

    #[test]
    fn runtime_port_takes_precedence_over_the_configured_port() {
        let home = daemon_test_home("runtime-port");
        std::fs::write(home.join("gravityd.toml"), "port = 49777\n").expect("configured port");
        std::fs::write(home.join("gravityd.port"), "50123\n").expect("runtime port");

        assert_eq!(daemon_port_from_home(&home), 50_123);
        std::fs::remove_dir_all(home).expect("test cleanup");
    }

    /// The dev script runs a workspace-private daemon as a plain child process
    /// on its own port, so the restart control must stay hidden there even
    /// when a managed install exists for the same home.
    #[test]
    fn only_the_managed_daemon_on_its_own_port_counts_as_managed() {
        let root = daemon_test_home("is-managed");
        let home = root.join("gravity");
        let user_home = root.join("user");
        let bin = home.join("bin/gravityd");
        let plist = user_home
            .join("Library/LaunchAgents")
            .join(format!("{LAUNCHD_LABEL}.plist"));
        std::fs::create_dir_all(bin.parent().expect("binary parent")).expect("binary directory");
        std::fs::create_dir_all(plist.parent().expect("plist parent")).expect("plist directory");
        std::fs::write(&bin, b"daemon").expect("daemon binary");
        std::fs::write(&plist, b"plist").expect("launchd plist");
        std::fs::write(home.join("gravityd.port"), "49777\n").expect("runtime port");

        assert!(managed_daemon_is_installed(&home, &user_home));
        assert_eq!(daemon_port_from_home(&home), 49_777);
        // The daemon the dev script starts answers elsewhere.
        assert_ne!(daemon_port_from_home(&home), 55_041);

        std::fs::remove_dir_all(root).expect("test cleanup");
    }

    #[test]
    fn invalid_runtime_port_falls_back_to_the_configured_port() {
        let home = daemon_test_home("invalid-runtime-port");
        std::fs::write(home.join("gravityd.toml"), "port = 7777\n").expect("configured port");
        std::fs::write(home.join("gravityd.port"), "stale\n").expect("runtime port");

        assert_eq!(daemon_port_from_home(&home), 7777);
        std::fs::remove_dir_all(home).expect("test cleanup");
    }
}
