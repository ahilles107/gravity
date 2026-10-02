//! A worker's checkout of the project's shared repository.
//!
//! Each machine keeps one bare clone per project as a cache. A worker gets a
//! worktree of it at `workspace/repo`, on a branch of its own, started from
//! the shared branch's tip as just fetched — so it always begins from the
//! latest pushed work, whichever machine pushed it. When the worker
//! completes its task, whatever it left is committed, rebased onto the
//! shared branch and pushed there; if that conflicts, it is pushed to the
//! worker's own branch instead, for its parent to merge.
//!
//! Git runs with this machine's own credentials, non-interactively, and every
//! operation on a project's cache is serialised: worktrees share its refs.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use bus::ProjectRepo;

use crate::worktree::{self, WorktreeSpec, METADATA_FILE};

/// Where a worker's checkout lives, relative to its workspace.
pub const CHECKOUT_DIR: &str = "repo";

/// Network operations (clone, fetch, push) give up after this long.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(300);
/// Everything else is local and quick.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(60);
/// Pushes rejected because another worker pushed first are retried.
const PUSH_ATTEMPTS: usize = 3;

/// The project's cache: a bare clone, here.
pub fn cache_dir(project_root: &Path) -> PathBuf {
    project_root.join("repo.git")
}

/// The worker's checkout, if it has one.
pub fn checkout_of(workspace: &Path) -> Option<PathBuf> {
    let dir = workspace.join(CHECKOUT_DIR);
    dir.join(METADATA_FILE).is_file().then_some(dir)
}

/// What came of pushing a worker's work.
#[derive(Debug, PartialEq, Eq)]
pub enum Published {
    Pushed {
        branch: String,
        commit: String,
    },
    /// The shared branch had moved on in a way that conflicts; the work went
    /// to the worker's own branch.
    Diverted {
        branch: String,
        onto: String,
    },
    Unchanged,
    Failed(String),
}

impl Published {
    /// One line for the result the parent reads.
    pub fn report(&self, checkout: &Path) -> String {
        match self {
            Self::Pushed { branch, commit } => {
                format!("repo: pushed {commit} to {branch}; pull to see it.")
            }
            Self::Diverted { branch, onto } => format!(
                "repo: this work conflicts with {onto}, so it was pushed to branch {branch} \
                 instead — merge that into {onto}."
            ),
            Self::Unchanged => "repo: no changes to push.".to_string(),
            Self::Failed(error) => format!(
                "repo: pushing failed ({error}); the work is still in {} on the worker's machine.",
                checkout.display()
            ),
        }
    }
}

/// Fetch the shared branch into the project's cache, cloning it first if
/// needed, and add a worktree for the worker at `workspace/repo`.
pub fn check_out(
    project_root: &Path,
    repo: &ProjectRepo,
    workspace: &Path,
    bot_id: &str,
    worker_name: &str,
) -> anyhow::Result<PathBuf> {
    let cache = cache_dir(project_root);
    let _held = lock(&cache);
    if !cache.join("HEAD").is_file() {
        clone(&cache, &repo.url)?;
    } else {
        git(
            &cache,
            &["remote", "set-url", "origin", &repo.url],
            LOCAL_TIMEOUT,
        )?;
    }
    fetch(&cache)?;
    let base = tracking(&repo.branch);
    git(
        &cache,
        &["rev-parse", "--verify", "--quiet", &base],
        LOCAL_TIMEOUT,
    )
    .with_context(|| format!("branch '{}' not found in {}", repo.branch, repo.url))?;
    // Checkouts of archived workers may be gone from disk by now.
    git(&cache, &["worktree", "prune"], LOCAL_TIMEOUT)?;
    let short: String = bot_id.chars().take(8).collect();
    worktree::provision(&WorktreeSpec {
        repo: cache.clone(),
        base_ref: base,
        dest: workspace.join(CHECKOUT_DIR),
        bot_id: bot_id.to_string(),
        bot_name: format!("{worker_name}-{short}"),
        copy_files: Vec::new(),
    })
}

/// Commit what the worker left, and push it to the shared branch — or to its
/// own branch when that conflicts.
pub fn publish(checkout: &Path, worker_name: &str, summary: &str) -> Published {
    match try_publish(checkout, worker_name, summary) {
        Ok(published) => published,
        Err(error) => Published::Failed(format!("{error:#}")),
    }
}

fn try_publish(checkout: &Path, worker_name: &str, summary: &str) -> anyhow::Result<Published> {
    let meta = worktree::read_meta(checkout)?;
    let _held = lock(&meta.repo);
    let target = meta
        .base_ref
        .strip_prefix("refs/remotes/origin/")
        .context("checkout does not track a shared branch")?
        .to_string();
    let own = git(checkout, &["branch", "--show-current"], LOCAL_TIMEOUT)?
        .trim()
        .to_string();

    if !git(checkout, &["status", "--porcelain"], LOCAL_TIMEOUT)?
        .trim()
        .is_empty()
    {
        git(checkout, &["add", "-A"], LOCAL_TIMEOUT)?;
        let message = format!("{worker_name}: {}", first_line(summary));
        as_worker(checkout, worker_name, &["commit", "-q", "-m", &message])?;
    }

    for _ in 0..PUSH_ATTEMPTS {
        fetch(&meta.repo)?;
        let ahead = git(
            checkout,
            &["rev-list", "--count", &format!("{}..HEAD", meta.base_ref)],
            LOCAL_TIMEOUT,
        )?;
        if ahead.trim() == "0" {
            return Ok(Published::Unchanged);
        }
        if as_worker(checkout, worker_name, &["rebase", &meta.base_ref]).is_err() {
            git(checkout, &["rebase", "--abort"], LOCAL_TIMEOUT).ok();
            return divert(checkout, &own, &target);
        }
        let refspec = format!("HEAD:refs/heads/{target}");
        if git(checkout, &["push", "origin", &refspec], NETWORK_TIMEOUT).is_ok() {
            let commit = git(checkout, &["rev-parse", "--short", "HEAD"], LOCAL_TIMEOUT)?;
            return Ok(Published::Pushed {
                branch: target,
                commit: commit.trim().to_string(),
            });
        }
        // Someone else pushed first: fetch their work and try again on top.
    }
    divert(checkout, &own, &target)
}

/// Push the worker's own branch, for its parent to merge.
fn divert(checkout: &Path, own: &str, target: &str) -> anyhow::Result<Published> {
    let refspec = format!("HEAD:refs/heads/{own}");
    git(
        checkout,
        &["push", "--force", "origin", &refspec],
        NETWORK_TIMEOUT,
    )?;
    Ok(Published::Diverted {
        branch: own.to_string(),
        onto: target.to_string(),
    })
}

/// Remove a retired worker's checkout once nothing in it is left unpushed.
/// Anything else is kept, never discarded.
pub fn retire(workspace: &Path) {
    let Some(checkout) = checkout_of(workspace) else {
        return;
    };
    let Ok(meta) = worktree::read_meta(&checkout) else {
        return;
    };
    let _held = lock(&meta.repo);
    match worktree::cleanup(&checkout) {
        Ok(worktree::CleanupOutcome::Removed) => {}
        Ok(worktree::CleanupOutcome::KeptDirty { detail }) => {
            tracing::info!(path = %checkout.display(), %detail, "kept a retired worker's checkout");
        }
        Err(error) => {
            tracing::warn!(path = %checkout.display(), %error, "removing a worker's checkout failed");
        }
    }
}

fn clone(cache: &Path, url: &str) -> anyhow::Result<()> {
    if cache.exists() {
        std::fs::remove_dir_all(cache).context("clearing a half-made repository cache")?;
    }
    let parent = cache.parent().context("cache has no parent directory")?;
    std::fs::create_dir_all(parent)?;
    let dest = cache.display().to_string();
    git(
        parent,
        &["clone", "--bare", "--", url, &dest],
        NETWORK_TIMEOUT,
    )
    .with_context(|| format!("cloning {url}"))?;
    // A bare clone keeps branches as its own; track them as remote branches
    // instead, so fetching never fights the worktrees' branches.
    git(
        cache,
        &[
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ],
        LOCAL_TIMEOUT,
    )?;
    let exclude = cache.join("info").join("exclude");
    std::fs::create_dir_all(cache.join("info"))?;
    std::fs::write(&exclude, format!("{METADATA_FILE}\n"))?;
    Ok(())
}

fn fetch(cache: &Path) -> anyhow::Result<()> {
    git(cache, &["fetch", "--prune", "origin"], NETWORK_TIMEOUT)
        .map(|_| ())
        .context("fetching the shared repository")
}

/// Run a git command that writes commits as the machine's git identity, or
/// as the worker when the machine has none.
fn as_worker(checkout: &Path, worker_name: &str, args: &[&str]) -> anyhow::Result<()> {
    let configured = git(checkout, &["config", "user.email"], LOCAL_TIMEOUT)
        .is_ok_and(|email| !email.trim().is_empty());
    let name = format!("user.name={worker_name} (Gravity worker)");
    let mut full = Vec::new();
    if !configured {
        full.extend(["-c", &name, "-c", "user.email=worker@gravity.invalid"]);
    }
    full.extend_from_slice(args);
    git(checkout, &full, LOCAL_TIMEOUT).map(|_| ())
}

fn tracking(branch: &str) -> String {
    format!("refs/remotes/origin/{branch}")
}

fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("work");
    line.trim().chars().take(72).collect()
}

/// One lock per cache: fetches, worktree changes and pushes all touch its
/// shared refs.
fn lock(cache: &Path) -> MutexGuard<'static, ()> {
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
fn git(dir: &Path, args: &[&str], timeout: Duration) -> anyhow::Result<String> {
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
