//! A worker's clone of the project's shared repository.
//!
//! The worker clones, pulls and pushes it itself, as its prompt tells it to:
//! in its own terminal, in parallel with every other worker, where a slow
//! clone holds up nobody and a rebase conflict is resolved by the bot that
//! wrote the work. The daemon steps in only when a worker retires, to keep
//! whatever it left unpushed: committed and pushed to the worker's own
//! branch, which its parent is told about.

use std::path::{Path, PathBuf};

use super::git::{git, LOCAL_TIMEOUT, NETWORK_TIMEOUT};

/// Where a worker clones the repository, relative to its workspace.
pub const CHECKOUT_DIR: &str = "repo";

/// The worker's clone, if it made one.
pub fn checkout_of(workspace: &Path) -> Option<PathBuf> {
    let dir = workspace.join(CHECKOUT_DIR);
    dir.join(".git").exists().then_some(dir)
}

/// The branch a worker's unpushed work is saved to.
pub fn salvage_branch(worker_name: &str, bot_id: &str) -> String {
    let name: String = worker_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase();
    let short: String = bot_id.chars().take(8).collect();
    format!("gravity/{name}-{short}")
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

/// Commit whatever a retiring worker left and push it to its own branch,
/// unless all of it is already on the remote.
pub fn salvage(checkout: &Path, worker_name: &str, bot_id: &str) -> Salvaged {
    match try_salvage(checkout, worker_name, bot_id) {
        Ok(salvaged) => salvaged,
        Err(error) => Salvaged::Failed(format!("{error:#}")),
    }
}

fn try_salvage(checkout: &Path, worker_name: &str, bot_id: &str) -> anyhow::Result<Salvaged> {
    let dirty = !git(checkout, &["status", "--porcelain"], LOCAL_TIMEOUT)?
        .trim()
        .is_empty();
    if dirty {
        git(checkout, &["add", "-A"], LOCAL_TIMEOUT)?;
        let message = format!("{worker_name}: unfinished work");
        as_worker(checkout, worker_name, &["commit", "-q", "-m", &message])?;
    }
    // A push updates the matching remote-tracking branch, so work the worker
    // pushed itself shows as on the remote without fetching again.
    let containing = git(
        checkout,
        &["branch", "-r", "--contains", "HEAD"],
        LOCAL_TIMEOUT,
    )?;
    if !containing.trim().is_empty() {
        return Ok(Salvaged::Nothing);
    }
    let branch = salvage_branch(worker_name, bot_id);
    let refspec = format!("HEAD:refs/heads/{branch}");
    git(
        checkout,
        &["push", "--force", "origin", &refspec],
        NETWORK_TIMEOUT,
    )?;
    Ok(Salvaged::Saved { branch })
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

#[cfg(test)]
mod tests {
    use super::salvage_branch;

    #[test]
    fn a_salvage_branch_is_the_workers_name_and_id() {
        assert_eq!(
            salvage_branch("Chapter 3", "0123456789ab"),
            "gravity/chapter-3-01234567"
        );
    }
}
