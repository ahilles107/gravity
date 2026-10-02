//! Running git for workers: non-interactively, with a timeout, and one
//! operation per project cache at a time.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{bail, Context};

/// Network operations (clone, fetch, push) give up after this long.
pub(super) const NETWORK_TIMEOUT: Duration = Duration::from_secs(300);
/// Everything else is local and quick.
pub(super) const LOCAL_TIMEOUT: Duration = Duration::from_secs(60);

/// One lock per cache: fetches, worktree changes and pushes all touch its
/// shared refs.
pub(super) fn lock(cache: &Path) -> MutexGuard<'static, ()> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, &'static Mutex<()>>>> = OnceLock::new();
    // One small lock per project for the life of the daemon, so its guard can
    // outlive the map's.
    let mut locks = LOCKS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let entry: &'static Mutex<()> = match locks.get(cache).copied() {
        Some(entry) => entry,
        None => {
            let entry: &'static Mutex<()> = Box::leak(Box::default());
            locks.insert(cache.to_path_buf(), entry);
            entry
        }
    };
    drop(locks);
    entry.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Run git non-interactively in `dir`, killing it past `timeout`.
pub(super) fn git(dir: &Path, args: &[&str], timeout: Duration) -> anyhow::Result<String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "protocol.ext.allow=never"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running git")?;
    // Drained while git runs, or a long listing would fill the pipe and
    // stall it until the timeout.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            if let Some(mut pipe) = pipe {
                pipe.read_to_end(&mut out).ok();
            }
            out
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > timeout {
            child.kill().ok();
            child.wait().ok();
            bail!("git {} timed out", args.first().unwrap_or(&""));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    if !status.success() {
        bail!(
            "git {} failed: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&stdout).to_string())
}
