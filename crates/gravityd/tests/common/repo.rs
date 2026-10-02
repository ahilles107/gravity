//! A project with a shared repository: a bare "remote" on disk and a book
//! bot that spawns workers against it.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

use super::peers::{bot_named, project};
use super::{create_bot, spawn_daemon_with, McpClient, TestDaemon, WsClient};

pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// A bare "remote" whose `main` holds one commit with a README.
pub fn remote(root: &Path) -> PathBuf {
    let bare = root.join("origin.git");
    let seed = root.join("seed");
    std::fs::create_dir_all(&seed).expect("seed dir");
    git(&seed, &["init", "-q", "-b", "main"]);
    std::fs::write(seed.join("README.md"), "# The book\n").expect("readme");
    git(&seed, &["add", "README.md"]);
    git(&seed, &["commit", "-q", "-m", "start"]);
    git(root, &["clone", "-q", "--bare", "seed", "origin.git"]);
    bare
}

pub struct Book {
    pub d: TestDaemon,
    pub pid: String,
    pub bus: McpClient,
    pub origin: PathBuf,
    pub _remote: tempfile::TempDir,
}

pub async fn book() -> Book {
    let remote_dir = tempfile::tempdir().expect("tempdir");
    let origin = remote(remote_dir.path());
    let d = spawn_daemon_with(|cfg| cfg.max_workers_per_project = 3).await;
    let mut c = WsClient::connect(&d).await;
    let pid = project(&mut c, "novel").await;
    let set = c
        .request(json!({
            "type": "set_project_repo", "project_id": pid,
            "url": origin.display().to_string(), "branch": "main"
        }))
        .await;
    assert_eq!(set["type"], "project", "{set}");
    assert_eq!(set["project"]["repo"]["branch"], "main");
    let bot = create_bot(&mut c, &pid, "book").await;
    let token = d
        .app
        .secrets
        .bot_token(bot["id"].as_str().expect("id"))
        .expect("token");
    Book {
        bus: McpClient::new(&d, &token),
        d,
        pid,
        origin,
        _remote: remote_dir,
    }
}

impl Book {
    pub async fn spawn(&mut self, name: &str) -> (Value, PathBuf, McpClient) {
        let spawned = self
            .bus
            .call(
                "spawn_worker",
                json!({ "name": name, "task": format!("Write {name}.") }),
            )
            .await;
        assert_eq!(spawned["state"], "running", "{spawned}");
        let bot = bot_named(&self.d, &self.pid, name).expect("worker");
        let checkout = PathBuf::from(&bot.workspace_path).join("repo");
        let token = self.d.app.secrets.bot_token(&bot.id).expect("token");
        (spawned, checkout, McpClient::new(&self.d, &token))
    }

    pub fn on_main(&self, path: &str) -> String {
        git(&self.origin, &["show", &format!("main:{path}")])
    }
}

pub async fn complete(worker: &mut McpClient, spawned: &Value, result: &str) -> Value {
    worker
        .call(
            "complete_task",
            json!({ "task_id": spawned["task_id"], "result": result }),
        )
        .await
}
