//! `gravityd service` — installs the daemon as a launchd user agent.
//!
//! Everything is user-domain so no sudo is ever needed: the binary is copied
//! to `<home>/bin/gravityd`, the agent plist goes to
//! `~/Library/LaunchAgents`, and logs live under `<home>/logs`. The desktop
//! app drives the same code path by running its bundled sidecar with
//! `service install`, so GUI and CLI installs are identical.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;

pub const LAUNCHD_LABEL: &str = "in.mikolajczuk.gravityd";
const DEFAULT_CONFIG: &str = include_str!("../../../ops/gravityd.example.toml");

/// Where the managed installation lives, derived from the daemon home
/// (`~/.gravity`) and the user home (for `~/Library/LaunchAgents`).
pub struct ServicePaths {
    home: PathBuf,
    user_home: PathBuf,
    launch_agents: PathBuf,
}

impl ServicePaths {
    pub fn new(home: PathBuf, user_home: PathBuf) -> Self {
        Self {
            launch_agents: user_home.join("Library/LaunchAgents"),
            home,
            user_home,
        }
    }

    pub fn bin_path(&self) -> PathBuf {
        self.home.join("bin").join("gravityd")
    }

    pub fn plist_path(&self) -> PathBuf {
        self.launch_agents.join(format!("{LAUNCHD_LABEL}.plist"))
    }

    pub fn config_path(&self) -> PathBuf {
        self.home.join("gravityd.toml")
    }

    pub fn log_dir(&self) -> PathBuf {
        self.home.join("logs")
    }

    pub fn runtime_port_path(&self) -> PathBuf {
        crate::home::runtime_port_path(&self.home)
    }
}

/// Renders the launchd agent plist with absolute paths baked in; launchd
/// expands neither `~` nor environment variables.
///
/// `PATH` has to cover wherever `claude` lives, because a launchd agent
/// inherits none of a login shell's environment and the daemon cannot spawn a
/// single bot without it. `~/.local/bin` is where Claude Code's own installer
/// puts it, so it is listed first; the Homebrew and system directories cover
/// the other install routes and the tools bots shell out to.
fn render_plist(binary: &Path, log_dir: &Path, user_home: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LAUNCHD_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>--negotiate-port</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>ThrottleInterval</key>
    <integer>10</integer>
    <key>StandardOutPath</key>
    <string>{out_log}</string>
    <key>StandardErrorPath</key>
    <string>{err_log}</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>{local_bin}:/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin</string>
    </dict>
</dict>
</plist>
"#,
        binary = binary.display(),
        out_log = log_dir.join("gravityd.out.log").display(),
        err_log = log_dir.join("gravityd.err.log").display(),
        local_bin = user_home.join(".local/bin").display(),
    )
}

/// Stages the installation on disk: binary, default config (kept if present),
/// and the agent plist. Does not touch launchd — see [`reload`].
pub fn install(source: &Path, paths: &ServicePaths) -> anyhow::Result<()> {
    let bin_path = paths.bin_path();
    let bin_dir = bin_path.parent().context("binary path has no parent")?;
    std::fs::create_dir_all(bin_dir).with_context(|| format!("creating {}", bin_dir.display()))?;
    std::fs::create_dir_all(paths.log_dir())?;
    std::fs::create_dir_all(&paths.launch_agents)?;

    let config_path = paths.config_path();
    if !config_path.exists() {
        std::fs::write(&config_path, DEFAULT_CONFIG)
            .with_context(|| format!("writing {}", config_path.display()))?;
    }

    // A rename atomically replaces the destination even while the old binary
    // is executing, which a plain overwrite of a running file would not.
    if source != bin_path {
        let staged = bin_dir.join("gravityd.new");
        std::fs::copy(source, &staged)
            .with_context(|| format!("copying {} to {}", source.display(), staged.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&staged, &bin_path)?;
    }

    let plist_path = paths.plist_path();
    std::fs::write(
        &plist_path,
        render_plist(&bin_path, &paths.log_dir(), &paths.user_home),
    )
    .with_context(|| format!("writing {}", plist_path.display()))?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        binary = %bin_path.display(),
        plist = %plist_path.display(),
        logs = %paths.log_dir().display(),
        "gravityd install staged"
    );
    Ok(())
}

/// Removes the agent plist and the managed binary. Daemon state (database,
/// config, secrets, bot workspaces) is deliberately left in place.
pub fn uninstall(paths: &ServicePaths) -> anyhow::Result<()> {
    tracing::info!(plist = %paths.plist_path().display(), "stopping gravityd agent");
    if let Err(e) = bootout(&paths.plist_path()) {
        tracing::warn!(error = %e, "bootout failed; removing files anyway");
    }
    // The published port goes with the service: state is kept, but nothing
    // should keep pointing clients at a port this machine no longer serves.
    for path in [
        paths.plist_path(),
        paths.bin_path(),
        paths.runtime_port_path(),
    ] {
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            tracing::info!(path = %path.display(), "removed");
        }
    }
    Ok(())
}

fn gui_domain() -> anyhow::Result<String> {
    let out = Command::new("id")
        .arg("-u")
        .output()
        .context("running id -u")?;
    let uid = String::from_utf8_lossy(&out.stdout).trim().to_string();
    anyhow::ensure!(!uid.is_empty(), "could not determine uid");
    Ok(format!("gui/{uid}"))
}

/// Stops the agent, logging launchctl's own verdict rather than discarding it.
fn bootout(plist: &Path) -> anyhow::Result<()> {
    let domain = gui_domain()?;
    let out = Command::new("launchctl")
        .args(["bootout", &domain])
        .arg(plist)
        .output()
        .context("running launchctl bootout")?;
    // A failure here is normal on a first install: nothing is loaded yet.
    tracing::info!(
        %domain,
        status = out.status.code().unwrap_or(-1),
        stderr = %String::from_utf8_lossy(&out.stderr).trim(),
        "launchctl bootout"
    );
    Ok(())
}

/// (Re)starts the agent: bootout is best-effort (the agent may not be
/// loaded), bootstrap must succeed.
pub fn reload(paths: &ServicePaths) -> anyhow::Result<()> {
    let plist = paths.plist_path();
    tracing::info!(plist = %plist.display(), "restarting gravityd agent");
    if let Err(e) = bootout(&plist) {
        tracing::warn!(error = %e, "bootout failed; continuing to bootstrap");
    }
    let domain = gui_domain()?;
    let out = Command::new("launchctl")
        .args(["bootstrap", &domain])
        .arg(&plist)
        .output()
        .context("running launchctl bootstrap")?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() {
        tracing::error!(
            %domain,
            status = out.status.code().unwrap_or(-1),
            stderr = %stderr.trim(),
            "launchctl bootstrap failed"
        );
    }
    anyhow::ensure!(
        out.status.success(),
        "launchctl bootstrap failed: {}",
        stderr.trim()
    );
    tracing::info!(%domain, label = LAUNCHD_LABEL, "gravityd agent started");
    Ok(())
}

/// Prints a human-readable status line per component and returns whether the
/// daemon looks fully installed and reachable.
pub fn status(paths: &ServicePaths, configured_port: u16) -> bool {
    let port = crate::home::runtime_port(&paths.home).unwrap_or(configured_port);
    let bin = paths.bin_path().exists();
    let plist = paths.plist_path().exists();
    // `/health`, not a bare connect: a published port outlives a daemon that
    // crashed, and reporting whoever picked it up as "reachable" is worse than
    // reporting nothing.
    let version = crate::server::probe_health(port, std::time::Duration::from_secs(2));
    let reachable = version.is_some();
    println!(
        "binary    {} ({})",
        if bin { "installed" } else { "missing" },
        paths.bin_path().display()
    );
    println!(
        "agent     {} ({})",
        if plist { "installed" } else { "missing" },
        paths.plist_path().display()
    );
    println!(
        "daemon    {} (127.0.0.1:{port}{})",
        match version {
            Some(ref v) => format!("reachable, version {v}"),
            None => "not reachable".to_string(),
        },
        if port == configured_port {
            String::new()
        } else {
            format!(", negotiated; {configured_port} was in use")
        }
    );
    if port != configured_port {
        println!(
            "warning   bots reach the bus at /mcp on {port}, not the configured {configured_port}"
        );
    }
    bin && plist && reachable
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(dir: &Path) -> ServicePaths {
        ServicePaths::new(dir.join("state"), dir.join("userhome"))
    }

    fn fake_binary(dir: &Path) -> PathBuf {
        let src = dir.join("gravityd-src");
        std::fs::write(&src, b"#!/bin/sh\n").unwrap();
        src
    }

    #[test]
    fn plist_bakes_absolute_paths() {
        let rendered = render_plist(
            Path::new("/x/bin/gravityd"),
            Path::new("/x/logs"),
            Path::new("/Users/x"),
        );
        assert!(rendered.contains("<string>/x/bin/gravityd</string>"));
        assert!(rendered.contains("<string>/x/logs/gravityd.out.log</string>"));
        assert!(rendered.contains("<string>--negotiate-port</string>"));
        assert!(rendered.contains(LAUNCHD_LABEL));
        assert!(!rendered.contains('~'));
    }

    /// A launchd agent inherits no login-shell environment, so a `claude`
    /// installed by Claude Code's own installer is only reachable if the plist
    /// puts `~/.local/bin` on PATH.
    #[test]
    fn plist_path_covers_the_claude_code_installer_location() {
        let rendered = render_plist(
            Path::new("/x/bin/gravityd"),
            Path::new("/x/logs"),
            Path::new("/Users/x"),
        );
        assert!(rendered.contains("<string>/Users/x/.local/bin:/usr/local/bin:"));
    }

    #[test]
    fn install_stages_binary_config_and_plist() {
        let tmp = tempfile::tempdir().unwrap();
        let p = paths(tmp.path());
        install(&fake_binary(tmp.path()), &p).unwrap();
        assert!(p.bin_path().exists());
        assert!(p.plist_path().exists());
        assert!(p.config_path().exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(p.bin_path())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o755, 0o755);
        }
    }

    #[test]
    fn install_keeps_an_existing_config() {
        let tmp = tempfile::tempdir().unwrap();
        let p = paths(tmp.path());
        std::fs::create_dir_all(p.config_path().parent().unwrap()).unwrap();
        std::fs::write(p.config_path(), "port = 9999\n").unwrap();
        install(&fake_binary(tmp.path()), &p).unwrap();
        assert_eq!(
            std::fs::read_to_string(p.config_path()).unwrap(),
            "port = 9999\n"
        );
    }

    #[test]
    fn install_replaces_a_previous_binary() {
        let tmp = tempfile::tempdir().unwrap();
        let p = paths(tmp.path());
        install(&fake_binary(tmp.path()), &p).unwrap();
        let src = tmp.path().join("gravityd-src");
        std::fs::write(&src, b"new contents").unwrap();
        install(&src, &p).unwrap();
        assert_eq!(std::fs::read(p.bin_path()).unwrap(), b"new contents");
    }

    #[test]
    fn install_from_the_installed_path_skips_the_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let p = paths(tmp.path());
        install(&fake_binary(tmp.path()), &p).unwrap();
        // Reinstalling from the managed location itself must not fail.
        install(&p.bin_path(), &p).unwrap();
        assert!(p.bin_path().exists());
    }
}
