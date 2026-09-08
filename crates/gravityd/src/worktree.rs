//! Git worktree provisioning for bots sharing a repository: explicit base
//! ref, ownership metadata, copied-file allowlist, and non-destructive
//! cleanup. Worktrees containing changes are never removed automatically.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};

pub const METADATA_FILE: &str = ".gravity-worktree.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeMeta {
    pub version: u32,
    pub bot_id: String,
    pub bot_name: String,
    pub repo: PathBuf,
    pub base_ref: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct WorktreeSpec {
    pub repo: PathBuf,
    pub base_ref: String,
    pub dest: PathBuf,
    pub bot_id: String,
    pub bot_name: String,
    /// Repo-relative paths copied from the main checkout into the worktree
    /// (untracked config like `.env`). Anything outside the allowlist is not
    /// copied.
    pub copy_files: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CleanupOutcome {
    Removed,
    /// The worktree has uncommitted or untracked changes and was kept.
    KeptDirty {
        detail: String,
    },
}

fn git(repo: &Path, args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .context("running git")?;
    if !out.status.success() {
        bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Create a worktree at `spec.dest` from `spec.base_ref`, on a bot-owned
/// branch, with ownership metadata and allowlisted file copies.
pub fn provision(spec: &WorktreeSpec) -> anyhow::Result<PathBuf> {
    if spec.dest.exists() {
        bail!(
            "worktree destination already exists: {}",
            spec.dest.display()
        );
    }
    // Resolve the base ref explicitly so a typo fails here, not later.
    git(&spec.repo, &["rev-parse", "--verify", &spec.base_ref])
        .with_context(|| format!("base ref '{}' not found", spec.base_ref))?;

    let branch = format!("gravity/{}", sanitize_branch(&spec.bot_name));
    if let Some(parent) = spec.dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    git(
        &spec.repo,
        &[
            "worktree",
            "add",
            "-b",
            &branch,
            &spec.dest.display().to_string(),
            &spec.base_ref,
        ],
    )?;

    for rel in &spec.copy_files {
        let rel_path = Path::new(rel);
        if rel_path.is_absolute() || rel_path.components().any(|c| c.as_os_str() == "..") {
            bail!("copy_files entries must be repo-relative without '..': {rel}");
        }
        let src = spec.repo.join(rel_path);
        if !src.exists() {
            continue;
        }
        let dst = spec.dest.join(rel_path);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src, &dst).with_context(|| format!("copying allowlisted file {rel}"))?;
    }

    let meta = WorktreeMeta {
        version: 1,
        bot_id: spec.bot_id.clone(),
        bot_name: spec.bot_name.clone(),
        repo: spec.repo.clone(),
        base_ref: spec.base_ref.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    std::fs::write(
        spec.dest.join(METADATA_FILE),
        serde_json::to_string_pretty(&meta)?,
    )?;
    Ok(spec.dest.clone())
}

pub fn read_meta(worktree: &Path) -> anyhow::Result<WorktreeMeta> {
    let raw = std::fs::read_to_string(worktree.join(METADATA_FILE))
        .context("reading worktree ownership metadata")?;
    Ok(serde_json::from_str(&raw)?)
}

/// Remove a worktree only when it is fully clean and was provisioned by
/// Gravity (ownership metadata present). Never force-removes.
pub fn cleanup(worktree: &Path) -> anyhow::Result<CleanupOutcome> {
    let meta =
        read_meta(worktree).context("refusing cleanup: not a Gravity-provisioned worktree")?;

    let status = git(worktree, &["status", "--porcelain"])?;
    // Our metadata file itself is expectedly untracked.
    let dirty: Vec<&str> = status
        .lines()
        .filter(|l| !l.ends_with(METADATA_FILE))
        .collect();
    if !dirty.is_empty() {
        return Ok(CleanupOutcome::KeptDirty {
            detail: format!("{} changed/untracked path(s)", dirty.len()),
        });
    }
    // Unpushed commits on the bot branch also block removal.
    let base = format!("{}..HEAD", meta.base_ref);
    let ahead = git(worktree, &["rev-list", "--count", &base]).unwrap_or_default();
    if ahead.trim() != "0" {
        return Ok(CleanupOutcome::KeptDirty {
            detail: format!(
                "{} unmerged commit(s) ahead of {}",
                ahead.trim(),
                meta.base_ref
            ),
        });
    }

    std::fs::remove_file(worktree.join(METADATA_FILE)).ok();
    git(
        &meta.repo,
        &["worktree", "remove", &worktree.display().to_string()],
    )?;
    Ok(CleanupOutcome::Removed)
}

fn sanitize_branch(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .arg("-C")
                .arg(p)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{:?}: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "-b", "main"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
        std::fs::write(p.join("file.txt"), "hello").unwrap();
        std::fs::write(p.join(".env"), "SECRET=1").unwrap();
        std::fs::write(p.join(".gitignore"), ".env\n").unwrap();
        run(&["add", "file.txt", ".gitignore"]);
        run(&["commit", "-m", "init"]);
        dir
    }

    fn spec(repo: &Path, dest: &Path) -> WorktreeSpec {
        WorktreeSpec {
            repo: repo.to_path_buf(),
            base_ref: "main".to_string(),
            dest: dest.to_path_buf(),
            bot_id: "b1".to_string(),
            bot_name: "alice".to_string(),
            copy_files: vec![".env".to_string()],
        }
    }

    #[test]
    fn provision_copies_allowlist_and_writes_meta() {
        let repo = init_repo();
        let dest = tempfile::tempdir().unwrap();
        let wt = dest.path().join("alice");
        provision(&spec(repo.path(), &wt)).unwrap();
        assert!(wt.join("file.txt").exists());
        assert!(wt.join(".env").exists(), "allowlisted file copied");
        assert!(wt.join(METADATA_FILE).exists());
        assert_eq!(
            git(&wt, &["branch", "--show-current"])
                .expect("current branch")
                .trim(),
            "gravity/alice"
        );
        let meta = read_meta(&wt).unwrap();
        assert_eq!(meta.bot_name, "alice");
        assert_eq!(meta.base_ref, "main");
    }

    #[test]
    fn provision_rejects_bad_base_ref_and_traversal() {
        let repo = init_repo();
        let dest = tempfile::tempdir().unwrap();
        let mut s = spec(repo.path(), &dest.path().join("wt"));
        s.base_ref = "no-such-ref".to_string();
        assert!(provision(&s).is_err());
        let mut s = spec(repo.path(), &dest.path().join("wt2"));
        s.copy_files = vec!["../outside".to_string()];
        assert!(provision(&s).is_err());
    }

    #[test]
    fn cleanup_keeps_dirty_worktrees() {
        let repo = init_repo();
        let dest = tempfile::tempdir().unwrap();
        let wt = dest.path().join("alice");
        provision(&spec(repo.path(), &wt)).unwrap();

        std::fs::write(wt.join("new.txt"), "work in progress").unwrap();
        match cleanup(&wt).unwrap() {
            CleanupOutcome::KeptDirty { .. } => {}
            other => panic!("expected KeptDirty, got {other:?}"),
        }
        assert!(wt.join("new.txt").exists(), "nothing was deleted");

        std::fs::remove_file(wt.join("new.txt")).unwrap();
        assert_eq!(cleanup(&wt).unwrap(), CleanupOutcome::Removed);
        assert!(!wt.exists());
    }

    #[test]
    fn cleanup_refuses_foreign_directories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("data.txt"), "not ours").unwrap();
        assert!(cleanup(dir.path()).is_err());
        assert!(dir.path().join("data.txt").exists());
    }
}
