//! Marking a daemon-created workspace as trusted in Claude Code's global
//! project state, and the cross-process lock that write has to respect.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Context;

use super::atomic_write_private_json;

static CLAUDE_CONFIG_LOCK: Mutex<()> = Mutex::new(());
/// How long Claude Code lets its config lock sit untouched before treating the
/// holder as dead. It refreshes the mtime while held, so anything older is a
/// process that died mid-write.
const CLAUDE_CONFIG_LOCK_STALE: Duration = Duration::from_secs(10);
const CLAUDE_CONFIG_LOCK_RETRY: Duration = Duration::from_millis(50);
/// Giving up is safe: the caller logs and the next start tries again.
const CLAUDE_CONFIG_LOCK_WAIT: Duration = Duration::from_millis(500);

/// Mark a daemon-created workspace as trusted in Claude Code's global project
/// state so its first interactive session does not stop at the trust dialog.
///
/// Keyed on the resolved workspace path, which is what Claude Code looks up —
/// except for a directory inside a git checkout, which it keys on the repo
/// root instead. A workspace under one would keep asking.
pub fn trust_workspace(user_home: &Path, workspace: &Path) -> anyhow::Result<()> {
    let _guard = CLAUDE_CONFIG_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("Claude config lock is poisoned"))?;
    let workspace = workspace
        .canonicalize()
        .with_context(|| format!("resolving workspace {}", workspace.display()))?;
    let workspace_key = workspace
        .to_str()
        .context("canonical workspace path is not valid UTF-8")?;
    let config_path = user_home.join(".claude.json");

    // The common case is a workspace that is already trusted. Read it unlocked:
    // taking the shared lock for a no-op would make a live session's own save
    // fail for nothing.
    if is_trusted(&read_claude_config(&config_path)?, workspace_key) {
        return Ok(());
    }

    // Writing means replacing the whole file, which belongs to every Claude
    // Code process on this machine, so the read the write is based on has to
    // happen under the lock.
    let _lock = ClaudeConfigLock::acquire(&config_path)?;
    let mut config = read_claude_config(&config_path)?;
    if is_trusted(&config, workspace_key) {
        return Ok(());
    }
    let root = config
        .as_object_mut()
        .context("Claude config root is not an object")?;
    let projects = root
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("Claude config projects field is not an object")?;
    let project = projects
        .entry(workspace_key)
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("Claude project state is not an object")?;
    project.insert(
        "hasTrustDialogAccepted".to_string(),
        serde_json::Value::Bool(true),
    );
    atomic_write_private_json(&config_path, &config)
}

/// A missing config is an empty one: Claude Code writes it on first run, and
/// the daemon may get there first.
fn read_claude_config(path: &Path) -> anyhow::Result<serde_json::Value> {
    match fs::read_to_string(path) {
        Ok(raw) => {
            serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::json!({})),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

fn is_trusted(config: &serde_json::Value, workspace_key: &str) -> bool {
    config["projects"][workspace_key]["hasTrustDialogAccepted"] == serde_json::Value::Bool(true)
}

/// The advisory lock Claude Code takes before saving `~/.claude.json`: a
/// `<config>.lock` *directory*, where `mkdir` is the atomic acquire and the
/// mtime is refreshed for as long as it is held. Holding the same one is what
/// keeps the rewrite in [`trust_workspace`] from dropping the edits of a
/// session that saved in the meantime.
struct ClaudeConfigLock {
    dir: PathBuf,
}

impl ClaudeConfigLock {
    fn acquire(config_path: &Path) -> anyhow::Result<Self> {
        // Claude Code resolves the config path before appending `.lock`, so a
        // symlinked home still names the same directory.
        let resolved = config_path.canonicalize();
        let dir = PathBuf::from(format!(
            "{}.lock",
            resolved.as_deref().unwrap_or(config_path).display()
        ));
        let deadline = Instant::now() + CLAUDE_CONFIG_LOCK_WAIT;
        loop {
            match fs::create_dir(&dir) {
                Ok(()) => return Ok(Self { dir }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(error).with_context(|| format!("locking {}", dir.display()));
                }
            }
            // Held. Break it if the holder died mid-write, otherwise wait.
            if is_stale_lock(&dir) {
                let _ = fs::remove_dir(&dir);
            }
            if Instant::now() >= deadline {
                anyhow::bail!(
                    "{} stayed locked by another Claude Code process",
                    config_path.display()
                );
            }
            std::thread::sleep(CLAUDE_CONFIG_LOCK_RETRY);
        }
    }
}

impl Drop for ClaudeConfigLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.dir);
    }
}

/// A lock nobody has touched for [`CLAUDE_CONFIG_LOCK_STALE`]. An unreadable
/// or future-dated mtime counts as fresh, so a clock jump cannot make the
/// daemon break a live session's lock.
fn is_stale_lock(dir: &Path) -> bool {
    fs::metadata(dir)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age > CLAUDE_CONFIG_LOCK_STALE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A temp home with a workspace and a `.claude.json` holding `config`.
    fn trust_fixture(config: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().expect("tmp");
        let workspace = tmp.path().join("workspace");
        fs::create_dir(&workspace).expect("workspace");
        let config_path = tmp.path().join(".claude.json");
        fs::write(&config_path, config).expect("config");
        let workspace = workspace.to_path_buf();
        (tmp, workspace, config_path)
    }

    /// The lock path exactly as `ClaudeConfigLock` derives it.
    fn config_lock_path(config_path: &Path) -> PathBuf {
        PathBuf::from(format!(
            "{}.lock",
            config_path
                .canonicalize()
                .expect("canonical config")
                .display()
        ))
    }

    #[test]
    fn trusting_workspace_preserves_existing_claude_config() {
        let tmp = tempfile::tempdir().expect("tmp");
        let workspace = tmp.path().join("workspace");
        fs::create_dir(&workspace).expect("workspace");
        let canonical_workspace = workspace.canonicalize().expect("canonical workspace");
        fs::write(
            tmp.path().join(".claude.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "theme": "dark",
                "projects": {
                    canonical_workspace.display().to_string(): {
                        "allowedTools": ["Read"],
                        "hasTrustDialogAccepted": false
                    },
                    "/other/workspace": { "hasTrustDialogAccepted": false }
                }
            }))
            .expect("json"),
        )
        .expect("config");

        trust_workspace(tmp.path(), &workspace).expect("trust");

        let config: serde_json::Value = serde_json::from_slice(
            &fs::read(tmp.path().join(".claude.json")).expect("read config"),
        )
        .expect("parse config");
        assert_eq!(config["theme"], "dark");
        assert_eq!(
            config["projects"][canonical_workspace.display().to_string()]["allowedTools"][0],
            "Read"
        );
        assert_eq!(
            config["projects"][canonical_workspace.display().to_string()]["hasTrustDialogAccepted"],
            true
        );
        assert_eq!(
            config["projects"]["/other/workspace"]["hasTrustDialogAccepted"],
            false
        );
    }

    /// The lock is Claude Code's, and a live session holding it is a session
    /// mid-write: the daemon must not replace the file underneath it.
    #[test]
    fn a_held_config_lock_defers_the_trust_write() {
        let (tmp, workspace, config_path) = trust_fixture(r#"{"theme":"dark"}"#);
        let lock = config_lock_path(&config_path);
        fs::create_dir(&lock).expect("hold lock");

        let result = trust_workspace(tmp.path(), &workspace);

        assert!(result.is_err(), "a held lock must defer the write");
        assert_eq!(
            fs::read_to_string(&config_path).expect("read config"),
            r#"{"theme":"dark"}"#,
            "the holder's file must be left alone"
        );
    }

    /// A lock left behind by a process that died mid-write would otherwise
    /// keep every later start out for good.
    #[test]
    fn a_stale_config_lock_is_broken() {
        let (tmp, workspace, config_path) = trust_fixture(r#"{"theme":"dark"}"#);
        let lock = config_lock_path(&config_path);
        fs::create_dir(&lock).expect("hold lock");
        let held = fs::File::open(&lock).expect("open lock");
        held.set_times(
            fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(3600)),
        )
        .expect("age the lock");

        trust_workspace(tmp.path(), &workspace).expect("trust");

        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(&config_path).expect("read config")).expect("parse");
        assert_eq!(config["theme"], "dark");
        assert!(is_trusted(
            &config,
            workspace
                .canonicalize()
                .expect("canonical")
                .to_str()
                .unwrap()
        ));
    }

    /// Every bot start re-asserts trust, and almost every one of those finds
    /// it already set. Taking the shared lock for that no-op would fail a
    /// concurrent session's save for nothing.
    #[test]
    fn an_already_trusted_workspace_never_takes_the_lock() {
        let tmp = tempfile::tempdir().expect("tmp");
        let workspace = tmp.path().join("workspace");
        fs::create_dir(&workspace).expect("workspace");
        let canonical = workspace.canonicalize().expect("canonical");
        let config_path = tmp.path().join(".claude.json");
        fs::write(
            &config_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "projects": {
                    canonical.display().to_string(): { "hasTrustDialogAccepted": true }
                }
            }))
            .expect("json"),
        )
        .expect("config");
        fs::create_dir(config_lock_path(&config_path)).expect("hold lock");

        trust_workspace(tmp.path(), &workspace).expect("trust");
    }

    #[test]
    fn trusting_workspace_creates_missing_claude_config_privately() {
        let tmp = tempfile::tempdir().expect("tmp");
        let workspace = tmp.path().join("workspace");
        fs::create_dir(&workspace).expect("workspace");
        let canonical_workspace = workspace.canonicalize().expect("canonical workspace");

        trust_workspace(tmp.path(), &workspace).expect("trust");

        let config_path = tmp.path().join(".claude.json");
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(&config_path).expect("read config"))
                .expect("parse config");
        assert_eq!(
            config["projects"][canonical_workspace.display().to_string()]["hasTrustDialogAccepted"],
            true
        );
        assert_eq!(
            fs::metadata(config_path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
