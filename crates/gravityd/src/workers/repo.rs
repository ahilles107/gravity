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

use std::path::{Path, PathBuf};

use anyhow::Context;
use bus::ProjectRepo;

use super::git::{git, lock, LOCAL_TIMEOUT, NETWORK_TIMEOUT};
use crate::worktree::{self, WorktreeSpec, METADATA_FILE};

/// Where a worker's checkout lives, relative to its workspace.
pub const CHECKOUT_DIR: &str = "repo";

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

/// What came of saving work a worker left unpushed.
#[derive(Debug, PartialEq, Eq)]
pub enum Salvaged {
    /// Everything it had is already on the remote.
    Nothing,
    Saved {
        branch: String,
    },
    Failed(String),
}

impl Salvaged {
    /// One line for the note its parent gets, or `None` when there is
    /// nothing to say.
    pub fn report(&self, checkout: &Path) -> Option<String> {
        match self {
            Self::Nothing => None,
            Self::Saved { branch } => Some(format!(
                "Work it had not pushed is saved on branch {branch} of the project \
                 repository; merge it if you want it."
            )),
            Self::Failed(error) => Some(format!(
                "Work it had not pushed could not be saved ({error}); it remains in {} on \
                 its machine.",
                checkout.display()
            )),
        }
    }
}

/// Commit whatever a retiring worker left and push it to the worker's own
/// branch, unless all of it is already on the remote — pushed to the shared
/// branch, or diverted to its own.
pub fn salvage(checkout: &Path, worker_name: &str) -> Salvaged {
    match try_salvage(checkout, worker_name) {
        Ok(salvaged) => salvaged,
        Err(error) => Salvaged::Failed(format!("{error:#}")),
    }
}

fn try_salvage(checkout: &Path, worker_name: &str) -> anyhow::Result<Salvaged> {
    let meta = worktree::read_meta(checkout)?;
    let _held = lock(&meta.repo);
    if !is_clean(checkout)? {
        git(checkout, &["add", "-A"], LOCAL_TIMEOUT)?;
        let message = format!("{worker_name}: unfinished work");
        as_worker(checkout, worker_name, &["commit", "-q", "-m", &message])?;
    }
    if is_on_remote(checkout)? {
        return Ok(Salvaged::Nothing);
    }
    let own = git(checkout, &["branch", "--show-current"], LOCAL_TIMEOUT)?
        .trim()
        .to_string();
    let refspec = format!("HEAD:refs/heads/{own}");
    git(
        checkout,
        &["push", "--force", "origin", &refspec],
        NETWORK_TIMEOUT,
    )?;
    Ok(Salvaged::Saved { branch: own })
}

fn is_clean(checkout: &Path) -> anyhow::Result<bool> {
    Ok(git(checkout, &["status", "--porcelain"], LOCAL_TIMEOUT)?
        .trim()
        .is_empty())
}

/// Whether HEAD is on some branch of the remote, as last fetched or pushed.
/// A push updates the matching remote-tracking branch, so this holds right
/// after a worker's own push.
fn is_on_remote(checkout: &Path) -> anyhow::Result<bool> {
    let containing = git(
        checkout,
        &["branch", "-r", "--contains", "HEAD"],
        LOCAL_TIMEOUT,
    )?;
    Ok(!containing.trim().is_empty())
}

/// Remove a retired worker's checkout, and its branch in the cache, once
/// everything in it is on the remote. Anything else is kept, never discarded.
pub fn retire(workspace: &Path) {
    let Some(checkout) = checkout_of(workspace) else {
        return;
    };
    if let Err(error) = try_retire(&checkout) {
        tracing::warn!(path = %checkout.display(), %error, "removing a worker's checkout failed");
    }
}

fn try_retire(checkout: &Path) -> anyhow::Result<()> {
    let meta = worktree::read_meta(checkout)?;
    let _held = lock(&meta.repo);
    if !is_clean(checkout)? || !is_on_remote(checkout)? {
        tracing::info!(path = %checkout.display(), "kept a retired worker's checkout: unpushed work");
        return Ok(());
    }
    let own = git(checkout, &["branch", "--show-current"], LOCAL_TIMEOUT)?;
    std::fs::remove_file(checkout.join(METADATA_FILE))?;
    let path = checkout.display().to_string();
    git(&meta.repo, &["worktree", "remove", &path], LOCAL_TIMEOUT)?;
    git(&meta.repo, &["branch", "-D", own.trim()], LOCAL_TIMEOUT)?;
    Ok(())
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
