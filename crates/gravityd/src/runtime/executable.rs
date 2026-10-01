//! Resolve user-installed CLIs without relying on a desktop-only PATH.
use std::process::Command;

pub(super) fn command(bin: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let path = if matches!(bin, "codex" | "codex.exe") {
            windows_codex(
                std::env::var_os("PATH").as_deref(),
                std::env::var_os("APPDATA").as_deref(),
                std::env::var_os("LOCALAPPDATA").as_deref(),
            )
        } else {
            None
        };
        let mut command = Command::new(path.unwrap_or_else(|| bin.into()));
        command.creation_flags(0x08000000);
        command
    }
    #[cfg(not(windows))]
    Command::new(bin)
}

pub(super) fn version(bin: &str) -> anyhow::Result<String> {
    use anyhow::Context;
    let out = command(bin)
        .arg("--version")
        .output()
        .with_context(|| format!("could not run {bin} --version"))?;
    anyhow::ensure!(out.status.success(), "{bin} --version failed");
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(windows)]
fn windows_codex(
    path: Option<&std::ffi::OsStr>,
    appdata: Option<&std::ffi::OsStr>,
    localappdata: Option<&std::ffi::OsStr>,
) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    // A CLI installed on PATH takes precedence over the desktop's bundled CLI.
    for directory in path.into_iter().flat_map(std::env::split_paths) {
        for name in ["codex.exe", "codex.cmd"] {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if let Some(appdata) = appdata {
        let npm = PathBuf::from(appdata).join("npm/codex.cmd");
        if npm.is_file() {
            return Some(npm);
        }
    }
    let local = PathBuf::from(localappdata?);
    for suffix in ["Programs/OpenAI/Codex/bin/codex.exe", "pnpm/codex.cmd"] {
        let installed = local.join(suffix);
        if installed.is_file() {
            return Some(installed);
        }
    }
    // Codex desktop uses versioned directories that are absent from the
    // Task Scheduler environment. Re-resolve on each start after app updates.
    let root = local.join("OpenAI/Codex/bin");
    std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let candidate = entry.path().join("codex.exe");
            let metadata = candidate.metadata().ok()?;
            metadata
                .is_file()
                .then_some((metadata.modified().ok()?, candidate))
        })
        .max_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)))
        .map(|(_, candidate)| candidate)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::path::Path;

    fn file(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "fixture").unwrap();
    }

    #[test]
    fn finds_desktop_codex_with_a_service_path_and_prefers_the_installed_cli() {
        let root = tempfile::tempdir().unwrap();
        let appdata = root.path().join("roaming");
        let local = root.path().join("local");
        let desktop = local.join("OpenAI/Codex/bin/build-a/codex.exe");
        file(&desktop);
        assert_eq!(
            windows_codex(None, Some(appdata.as_os_str()), Some(local.as_os_str())),
            Some(desktop)
        );
        let cli = local.join("Programs/OpenAI/Codex/bin/codex.exe");
        file(&cli);
        assert_eq!(
            windows_codex(None, Some(appdata.as_os_str()), Some(local.as_os_str())),
            Some(cli)
        );
        let npm = appdata.join("npm/codex.cmd");
        file(&npm);
        assert_eq!(
            windows_codex(None, Some(appdata.as_os_str()), Some(local.as_os_str())),
            Some(npm)
        );
        let installed = root.path().join("cli/codex.exe");
        file(&installed);
        let path = std::env::join_paths([installed.parent().unwrap()]).unwrap();
        assert_eq!(
            windows_codex(
                Some(&path),
                Some(appdata.as_os_str()),
                Some(local.as_os_str())
            ),
            Some(installed)
        );
        assert_eq!(windows_codex(None, None, None), None);
    }

    #[test]
    fn explicit_executable_paths_are_preserved() {
        assert_eq!(
            command("C:/custom/codex.exe").get_program(),
            "C:/custom/codex.exe"
        );
    }

    #[test]
    #[ignore = "requires installed Codex CLI; no model request"]
    fn installed_codex_runs_without_the_desktop_path() {
        let bin = windows_codex(None, None, std::env::var_os("LOCALAPPDATA").as_deref())
            .expect("installed Codex CLI");
        let out = command(&bin.to_string_lossy())
            .arg("--version")
            .env("PATH", "C:/Windows/System32;C:/Windows")
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        assert!(String::from_utf8_lossy(&out.stdout).contains("codex"));
    }
}
