//! Per-user Task Scheduler installation; no administrator privileges required.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::Context;

pub const SERVICE_LABEL: &str = "Gravity";
const DEFAULT_CONFIG: &str = include_str!("../../../../ops/gravityd.example.toml");

pub struct ServicePaths {
    home: PathBuf,
}

impl ServicePaths {
    pub fn new(home: PathBuf, _user_home: PathBuf) -> Self {
        Self { home }
    }
    pub fn bin_path(&self) -> PathBuf {
        self.home.join("bin/gravityd.exe")
    }
    // Kept as the common installation-marker API for the existing CLI.
    pub fn plist_path(&self) -> PathBuf {
        self.home.join("gravityd-task.xml")
    }
    pub fn config_path(&self) -> PathBuf {
        self.home.join("gravityd.toml")
    }
    pub fn log_dir(&self) -> PathBuf {
        self.home.join("logs")
    }
    fn launcher_path(&self) -> PathBuf {
        self.home.join("gravityd-task.ps1")
    }
    fn pid_path(&self) -> PathBuf {
        self.home.join("gravityd-task.pid")
    }
}

fn task_name(paths: &ServicePaths) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let home = paths.home.canonicalize()?.to_string_lossy().to_lowercase();
    let hash = hex::encode(Sha256::digest(home.as_bytes()));
    Ok(format!(
        "{SERVICE_LABEL}-{}-{}",
        crate::permissions::user_sid()?,
        &hash[..12]
    ))
}

fn quote(path: &Path) -> String {
    format!(
        "'{}'",
        path.to_string_lossy()
            .replace('/', "\\")
            .replace('\'', "''")
    )
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn render_task(paths: &ServicePaths, sid: &str) -> String {
    let arguments = format!(
        "-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File \"{}\"",
        paths.launcher_path().display()
    );
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{sid}</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="User"><UserId>{sid}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><AllowStartOnDemand>true</AllowStartOnDemand><Enabled>true</Enabled><Hidden>true</Hidden><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><RestartOnFailure><Interval>PT1M</Interval><Count>999</Count></RestartOnFailure></Settings>
  <Actions Context="User"><Exec><Command>powershell.exe</Command><Arguments>{arguments}</Arguments></Exec></Actions>
</Task>
"#,
        sid = xml(sid),
        arguments = xml(&arguments)
    )
}

fn run_task(args: &[&str]) -> anyhow::Result<()> {
    let out = Command::new("schtasks.exe")
        .args(args)
        .output()
        .context("running Task Scheduler")?;
    anyhow::ensure!(
        out.status.success(),
        "Task Scheduler failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(())
}

fn stop(paths: &ServicePaths) -> anyhow::Result<()> {
    let name = task_name(paths)?;
    // Ending an idle task returns an error; the home lock below verifies stop.
    let _ = Command::new("schtasks.exe")
        .args(["/end", "/tn", &name])
        .output()?;
    if let Ok(pid) = std::fs::read_to_string(paths.pid_path()) {
        let pid: u32 = pid.trim().parse().context("invalid managed daemon PID")?;
        // Never kill a reused PID belonging to another executable.
        let script = format!(
            "$p = Get-Process -Id {pid} -ErrorAction SilentlyContinue; if ($p -and $p.Path -eq {}) {{ & taskkill.exe /PID {pid} /T /F | Out-Null; exit $LASTEXITCODE }}",
            quote(&paths.bin_path())
        );
        let out = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()?;
        anyhow::ensure!(out.status.success(), "could not stop managed daemon");
        std::fs::remove_file(paths.pid_path())?;
    }
    // /end returns before Task Scheduler has finished ending the launcher.
    // /run during that interval reports success but IgnoreNew drops the run.
    let script = format!(
        "$s = New-Object -ComObject Schedule.Service; $s.Connect(); $t = $s.GetFolder('\\').GetTask('{name}'); $until = [DateTime]::UtcNow.AddSeconds(30); while ($t.State -ne 3) {{ if ([DateTime]::UtcNow -ge $until) {{ exit 1 }}; Start-Sleep -Milliseconds 100 }}"
    );
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()?;
    anyhow::ensure!(out.status.success(), "managed task did not finish stopping");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match crate::home::lock(&paths.home) {
            Ok(_lock) => return Ok(()),
            Err(error) if Instant::now() >= deadline => {
                return Err(error).context("waiting for the managed daemon to stop")
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

pub fn install(source: &Path, paths: &ServicePaths) -> anyhow::Result<()> {
    std::fs::create_dir_all(paths.home.join("bin"))?;
    std::fs::create_dir_all(paths.log_dir())?;
    if paths.plist_path().exists() {
        stop(paths)?;
    }
    if !paths.config_path().exists() {
        std::fs::write(paths.config_path(), DEFAULT_CONFIG)?;
    }
    if source != paths.bin_path() {
        std::fs::copy(source, paths.bin_path()).context("installing daemon binary")?;
    }
    let launcher = format!(
        "$ErrorActionPreference = 'Stop'\n$env:GRAVITY_HOME = {}\n$p = Start-Process -FilePath {} -ArgumentList '--negotiate-port' -WindowStyle Hidden -PassThru -RedirectStandardOutput {} -RedirectStandardError {}\n[IO.File]::WriteAllText({}, [string]$p.Id)\n$p.WaitForExit()\nexit $p.ExitCode\n",
        quote(&paths.home), quote(&paths.bin_path()), quote(&paths.log_dir().join("gravityd.out.log")), quote(&paths.log_dir().join("gravityd.err.log")), quote(&paths.pid_path())
    );
    std::fs::write(paths.launcher_path(), launcher)?;
    let sid = crate::permissions::user_sid()?;
    // schtasks reads task definitions as UTF-16, matching its own exports.
    let definition: Vec<u8> = std::iter::once(0xfeff)
        .chain(render_task(paths, &sid).encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    std::fs::write(paths.plist_path(), definition)?;
    let marker = paths.plist_path();
    run_task(&[
        "/create",
        "/tn",
        &task_name(paths)?,
        "/xml",
        &marker.to_string_lossy(),
        "/f",
    ])
}

pub fn reload(paths: &ServicePaths) -> anyhow::Result<()> {
    anyhow::ensure!(
        paths.plist_path().is_file(),
        "no managed daemon is installed"
    );
    stop(paths)?;
    run_task(&["/run", "/tn", &task_name(paths)?])
}

pub fn uninstall(paths: &ServicePaths) -> anyhow::Result<()> {
    stop(paths)?;
    run_task(&["/delete", "/tn", &task_name(paths)?, "/f"])?;
    for path in [
        paths.plist_path(),
        paths.launcher_path(),
        paths.bin_path(),
        paths.pid_path(),
        crate::home::runtime_port_path(&paths.home),
    ] {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub fn status(paths: &ServicePaths, configured_port: u16) -> bool {
    let port = crate::home::runtime_port(&paths.home).unwrap_or(configured_port);
    let installed = paths.bin_path().is_file() && paths.plist_path().is_file();
    let version = crate::server::probe_health(port, Duration::from_secs(2));
    println!(
        "task: {}; daemon: {} (127.0.0.1:{port})",
        if installed { "installed" } else { "missing" },
        version.as_deref().unwrap_or("unreachable")
    );
    installed && version.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_is_scoped_to_current_user_and_escapes_paths() {
        let paths = ServicePaths::new(
            PathBuf::from(r"C:\Users\Test & User\.gravity"),
            PathBuf::new(),
        );
        let task = render_task(&paths, "S-1-5-21-123");
        assert!(task.contains("InteractiveToken"));
        assert!(task.contains("LeastPrivilege"));
        assert!(task.contains("Test &amp; User"));
        assert!(task.contains("-WindowStyle Hidden"));
        assert!(task.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(task.contains("<UserId>S-1-5-21-123</UserId>"));
    }

    #[test]
    fn launcher_quotes_apostrophes() {
        assert_eq!(
            quote(Path::new("C:/O'Brien/gravity.exe")),
            r"'C:\O''Brien\gravity.exe'"
        );
    }
}
