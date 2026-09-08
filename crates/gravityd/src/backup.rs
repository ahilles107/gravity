//! Online backup and restore: SQLite backup API plus configuration
//! manifests. Secrets are never included.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context};

use crate::config::Config;
use crate::db::Db;

/// Create a backup directory containing `bus.sqlite`, `gravityd.toml` (if any),
/// per-project/bot config files, and a manifest. Secrets and workspaces are
/// excluded.
pub fn backup(cfg: &Config, db: &Db, out: &Path) -> anyhow::Result<PathBuf> {
    if out.exists() && out.read_dir()?.next().is_some() {
        bail!("backup destination is not empty: {}", out.display());
    }
    std::fs::create_dir_all(out)?;

    db.backup_to(&out.join("bus.sqlite"))
        .context("SQLite online backup")?;

    let cfg_path = cfg.home.join("gravityd.toml");
    if cfg_path.exists() {
        std::fs::copy(&cfg_path, out.join("gravityd.toml"))?;
    }

    // Bot configuration files, not workspaces and never `secrets/`.
    let mut copied = 0usize;
    let projects_src = cfg.projects_dir();
    if projects_src.exists() {
        copied = copy_config_tree(&projects_src, &out.join("projects"))?;
    }

    let manifest = serde_json::json!({
        "version": 1,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "daemon_version": crate::app::DAEMON_VERSION,
        "config_files": copied,
        "includes_secrets": false
    });
    std::fs::write(
        out.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(out.to_path_buf())
}

const CONFIG_FILES: &[&str] = &["project.json", "bot.json", "system.md", "mcp.json"];

fn copy_config_tree(src: &Path, dst: &Path) -> anyhow::Result<usize> {
    let mut copied = 0;
    for entry in walk(src)? {
        let name = entry
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !CONFIG_FILES.contains(&name) {
            continue;
        }
        let rel = entry.strip_prefix(src)?;
        let target = dst.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&entry, &target)?;
        copied += 1;
    }
    Ok(copied)
}

fn walk(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let path = entry?.path();
            if path.is_dir() {
                // Workspaces hold user data, not configuration; skip them.
                if path.file_name().and_then(|n| n.to_str()) == Some("workspace") {
                    continue;
                }
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    Ok(out)
}

/// Restore a backup into the daemon home. Refuses to run while a live
/// database exists unless `overwrite` is set; the existing database is moved
/// aside, never deleted.
pub fn restore(cfg: &Config, from: &Path, overwrite: bool) -> anyhow::Result<()> {
    let src_db = from.join("bus.sqlite");
    if !src_db.exists() {
        bail!("{} does not contain bus.sqlite", from.display());
    }
    // Validate the backup before touching anything.
    let check = Db::open(&src_db).context("opening backup database")?;
    if !check.integrity_check()? {
        bail!("backup database failed integrity check");
    }
    drop(check);

    let dest_db = cfg.db_path();
    if dest_db.exists() {
        if !overwrite {
            bail!(
                "{} already exists; pass --overwrite to move it aside and restore",
                dest_db.display()
            );
        }
        let aside = cfg.home.join(format!(
            "bus.sqlite.pre-restore-{}",
            chrono::Utc::now().timestamp()
        ));
        std::fs::rename(&dest_db, &aside)
            .with_context(|| format!("moving current db aside to {}", aside.display()))?;
        // WAL/SHM sidecars belong to the old database.
        for ext in ["-wal", "-shm"] {
            let side = cfg.home.join(format!("bus.sqlite{ext}"));
            if side.exists() {
                std::fs::rename(&side, cfg.home.join(format!("bus.sqlite{ext}.pre-restore")))?;
            }
        }
    }
    std::fs::create_dir_all(&cfg.home)?;
    std::fs::copy(&src_db, &dest_db)?;

    let projects_src = from.join("projects");
    if projects_src.exists() {
        for entry in walk(&projects_src)? {
            let rel = entry.strip_prefix(&projects_src)?;
            let target = cfg.projects_dir().join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&entry, &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn backup_and_restore_roundtrip() {
        let home = tempfile::tempdir().unwrap();
        let cfg = Config {
            home: home.path().to_path_buf(),
            ..Config::default()
        };
        let db = Db::open(&cfg.db_path()).unwrap();
        let project = db.create_project("acme", "acme").unwrap();
        db.create_bot(&project.id, "alice", "", "", "", "/tmp/a", "alice", None)
            .unwrap();

        // Fake provisioned config + a secret that must not travel.
        let bot_dir = cfg.projects_dir().join("acme/bots/alice");
        std::fs::create_dir_all(bot_dir.join("workspace")).unwrap();
        std::fs::write(bot_dir.join("bot.json"), "{}").unwrap();
        std::fs::write(bot_dir.join("workspace/CLAUDE.md"), "private").unwrap();
        std::fs::create_dir_all(cfg.secrets_dir()).unwrap();
        std::fs::write(cfg.secrets_dir().join("client.token"), "s3cret").unwrap();

        let out = home.path().join("backups/b1");
        backup(&cfg, &db, &out).unwrap();
        assert!(out.join("bus.sqlite").exists());
        assert!(out.join("manifest.json").exists());
        assert!(out.join("projects/acme/bots/alice/bot.json").exists());
        assert!(!out.join("projects/acme/bots/alice/workspace").exists());
        // No secrets anywhere in the backup.
        for f in walk(&out).unwrap() {
            let content = std::fs::read_to_string(&f).unwrap_or_default();
            assert!(!content.contains("s3cret"), "secret leaked into {f:?}");
        }

        // Restore into a fresh home.
        let home2 = tempfile::tempdir().unwrap();
        let cfg2 = Config {
            home: home2.path().to_path_buf(),
            ..Config::default()
        };
        restore(&cfg2, &out, false).unwrap();
        let db2 = Db::open(&cfg2.db_path()).unwrap();
        let projects = db2.list_projects().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "acme");
        assert_eq!(db2.list_bots(None).unwrap().len(), 1);

        // Existing database blocks restore without --overwrite.
        assert!(restore(&cfg2, &out, false).is_err());
        restore(&cfg2, &out, true).unwrap();
        assert!(std::fs::read_dir(home2.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e
                .file_name()
                .to_string_lossy()
                .starts_with("bus.sqlite.pre-restore")));
    }
}
