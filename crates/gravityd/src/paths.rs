//! On-disk provisioning of projects and bots per the architecture plan §4.4.
//! Versioned files are the source of truth for user-editable configuration;
//! SQLite owns runtime and delivery state.
//!
//! Two files carry a bot's prompt, and the split matters:
//!
//! - `system.md` is **daemon-owned**. It holds the name, description and
//!   instructions, is regenerated from the database on every identity change,
//!   and is injected with `--append-system-prompt` at spawn.
//! - `workspace/CLAUDE.md` is **bot-owned**. It is written once at creation and
//!   never rewritten, because the bot maintains it as living context. It
//!   imports `workspace/FACTS.md`, seeded the same way, which is where the bot
//!   parks facts that must outlive a compaction.
//!
//! Before the split, instructions lived in `CLAUDE.md`: editing them either did
//! nothing (the daemon only wrote the file at creation) or would have clobbered
//! the bot's own memory. Keeping the two apart is what makes instructions
//! editable at all.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::config::Config;

mod prompt;
#[cfg(test)]
mod tests;
mod trust;

pub use prompt::system_md;
pub use trust::trust_workspace;

#[derive(Debug, Serialize, Deserialize)]
pub struct BotFile {
    pub version: u32,
    pub id: String,
    pub name: String,
    /// Directory this bot lives in. Recorded because a rename changes `name`
    /// but deliberately leaves the directory alone.
    pub dir_name: String,
    pub project: String,
    pub created_at: String,
}

pub struct BotDirs {
    pub root: PathBuf,
    pub workspace: PathBuf,
}

/// Root directory for a project, keyed on its immutable `dir_name` rather than
/// its current name: renaming a project must not move the bots inside it.
pub fn project_dir(cfg: &Config, project_dir_name: &str) -> PathBuf {
    cfg.projects_dir().join(project_dir_name)
}

/// Root directory for a bot, keyed on its immutable `dir_name` rather than its
/// current name.
pub fn bot_dir(cfg: &Config, project_dir_name: &str, dir_name: &str) -> PathBuf {
    project_dir(cfg, project_dir_name)
        .join("bots")
        .join(dir_name)
}

/// Shared, project-scoped directory every bot may read and write. Substance
/// travels here as files; message bodies carry summaries and paths.
pub fn artifacts_dir(cfg: &Config, project_dir_name: &str) -> PathBuf {
    project_dir(cfg, project_dir_name).join("artifacts")
}

pub fn provision_project(
    cfg: &Config,
    project_id: &str,
    dir_name: &str,
    name: &str,
) -> anyhow::Result<()> {
    let dir = project_dir(cfg, dir_name);
    fs::create_dir_all(dir.join("bots"))?;
    fs::create_dir_all(dir.join("artifacts"))?;
    write_project_manifest(cfg, project_id, dir_name, name)
}

/// (Re)write `project.json`. Also called after a rename, so the manifest on
/// disk never disagrees with the database about what the project is called.
pub fn write_project_manifest(
    cfg: &Config,
    project_id: &str,
    dir_name: &str,
    name: &str,
) -> anyhow::Result<()> {
    let dir = project_dir(cfg, dir_name);
    let manifest = serde_json::json!({
        "version": 1, "id": project_id, "name": name, "dir_name": dir_name
    });
    atomic_write_json(&dir.join("project.json"), &manifest)
}

/// Update the `project` field of a bot's `bot.json` in place.
///
/// Rewriting the whole file would reset `created_at`, which is the one field
/// only the original provisioning knows. A bot whose file is missing or
/// unreadable is skipped: the manifest is descriptive, and a rename should not
/// fail over it.
pub fn set_bot_project_name(root: &Path, project_name: &str) -> anyhow::Result<()> {
    let path = root.join("bot.json");
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(());
    };
    let Ok(mut manifest) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Ok(());
    };
    let Some(object) = manifest.as_object_mut() else {
        return Ok(());
    };
    object.insert("project".to_string(), project_name.into());
    atomic_write_json(&path, &manifest)
}

/// Everything the daemon needs to lay out or refresh a bot's files.
pub struct BotProvision<'a> {
    pub project_name: &'a str,
    /// The project's frozen directory name; `project_name` is display only.
    pub project_dir_name: &'a str,
    pub bot_id: &'a str,
    pub name: &'a str,
    pub dir_name: &'a str,
    pub description: &'a str,
    pub instructions: &'a str,
    pub daemon_port: u16,
    pub bot_token_env: &'a str,
    /// Stated verbatim in the generated prompt so bots can budget against it.
    pub max_bots_per_project: usize,
    /// Absolute path of the project's shared artifacts directory, stated in
    /// the prompt so bots know where substance goes.
    pub artifacts_dir: String,
}

/// Create the bot's directory tree: `bot.json`, `system.md`, `mcp.json`, and a
/// workspace with `CLAUDE.md`, `FACTS.md` and cooperative `.claude/settings.json`
/// (permissions plus lifecycle hooks that report state to the daemon).
pub fn provision_bot(cfg: &Config, spec: &BotProvision<'_>) -> anyhow::Result<BotDirs> {
    let root = bot_dir(cfg, spec.project_dir_name, spec.dir_name);
    let workspace = root.join("workspace");
    fs::create_dir_all(workspace.join(".claude").join("skills"))?;

    let bot_file = BotFile {
        version: 1,
        id: spec.bot_id.to_string(),
        name: spec.name.to_string(),
        dir_name: spec.dir_name.to_string(),
        project: spec.project_name.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    atomic_write_json(&root.join("bot.json"), &serde_json::to_value(&bot_file)?)?;

    write_system_md(&root, spec)?;

    write_mcp_config(&root, spec.daemon_port, spec.bot_token_env)?;

    seed_memory_files(&workspace, spec.name)?;

    write_hook_settings(&workspace, spec.daemon_port, spec.bot_token_env)?;

    Ok(BotDirs { root, workspace })
}

/// (Re)write the bot's `mcp.json`. Called at creation and again on every start,
/// so a change to the daemon's port reaches existing bots without
/// re-provisioning — the same reason `write_hook_settings` runs each start. The
/// token is referenced through an environment variable the daemon injects when
/// it starts the runtime.
pub fn write_mcp_config(root: &Path, daemon_port: u16, bot_token_env: &str) -> anyhow::Result<()> {
    let mcp = serde_json::json!({
        "mcpServers": {
            "gravity-bus": {
                "type": "http",
                "url": format!("http://127.0.0.1:{daemon_port}/mcp"),
                "headers": {
                    "Authorization": format!("Bearer ${{{bot_token_env}}}")
                }
            }
        }
    });
    atomic_write_json(&root.join("mcp.json"), &mcp)
}

/// Rewrite `system.md` after an identity change. Cheap and idempotent, so it is
/// also safe to run for every bot at startup to migrate rows provisioned before
/// instructions moved into this file.
pub fn write_system_md(root: &Path, spec: &BotProvision<'_>) -> anyhow::Result<()> {
    let content = prompt::system_md(spec);
    let path = root.join("system.md");
    // Skip the write when nothing changed, so startup regeneration does not
    // churn mtimes across every bot on every boot.
    if let Ok(existing) = fs::read_to_string(&path) {
        if existing == content {
            return Ok(());
        }
    }
    fs::create_dir_all(root)?;
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, content).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, &path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}

/// Seed the bot-owned memory files: `CLAUDE.md` (living context) and the
/// `FACTS.md` it imports. Each is written only when missing, so this is safe to
/// call on every start — that is how a bot provisioned before `FACTS.md`
/// existed gets one without its own writing ever being clobbered.
pub fn seed_memory_files(workspace: &Path, name: &str) -> anyhow::Result<()> {
    for (file, body) in [
        ("CLAUDE.md", prompt::claude_md(name)),
        ("FACTS.md", prompt::facts_md(name)),
    ] {
        let path = workspace.join(file);
        if !path.exists() {
            fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
        }
    }
    Ok(())
}

/// (Re)write the workspace's cooperative `.claude/settings.json`. Also called
/// on every bot start so existing bots pick up hook/setting changes.
pub fn write_hook_settings(
    workspace: &Path,
    daemon_port: u16,
    bot_token_env: &str,
) -> anyhow::Result<()> {
    let dir = workspace.join(".claude");
    fs::create_dir_all(&dir)?;
    let settings = hook_settings(daemon_port, bot_token_env);
    atomic_write_json(&dir.join("settings.json"), &settings)
}

/// Permission rules granting access to the project's shared artifacts
/// directory — the one place outside the workspace a bot may read and write.
/// Passed to the runtime as `--allowedTools` rather than written into the
/// workspace's `settings.json`: a grant on the command line comes from the
/// daemon that spawned the session, so Claude Code's trust dialog never has
/// to warn that "this folder pre-approves permissions". Absolute paths so the
/// rules cannot be re-anchored by a cwd change.
///
/// `Edit(path)` covers every file-editing tool, `Write` included. Granting
/// `Write(path)` as well makes Claude Code print a warning at startup that it
/// is not matched by file permission checks.
pub fn artifacts_allow_rules(artifacts_dir: &Path) -> Vec<String> {
    let dir = artifacts_dir.display();
    vec![format!("Read({dir}/**)"), format!("Edit({dir}/**)")]
}

/// Cooperative permission rules and lifecycle hooks. Hooks POST lifecycle
/// events to the daemon; failures are swallowed (`|| true`) so a daemon
/// hiccup never blocks the session, and hook failure is never interpreted as
/// approval or denial.
fn hook_settings(daemon_port: u16, bot_token_env: &str) -> serde_json::Value {
    let hook_cmd = |event: &str| {
        serde_json::json!([{
            "hooks": [{
                "type": "command",
                "command": format!(
                    "curl -fsS -m 3 -X POST http://127.0.0.1:{daemon_port}/hook \
                     -H \"Authorization: Bearer ${{{bot_token_env}}}\" \
                     -H 'Content-Type: application/json' \
                     -d '{{\"event\":\"{event}\"}}' >/dev/null 2>&1 || true"
                )
            }]
        }])
    };
    // Notification carries Claude Code's stdin payload through untouched, so
    // the daemon can read its `message` and tell a permission prompt apart
    // from the "waiting for your input" idle ping. The event name rides in the
    // query string because the body is no longer ours to shape.
    let notification = serde_json::json!([{
        "hooks": [{
            "type": "command",
            "command": format!(
                "curl -fsS -m 3 -X POST \
                 'http://127.0.0.1:{daemon_port}/hook?event=Notification' \
                 -H \"Authorization: Bearer ${{{bot_token_env}}}\" \
                 -H 'Content-Type: application/json' --data-binary @- \
                 >/dev/null 2>&1 || true"
            )
        }]
    }]);
    // SessionStart additionally reports the session's inbox socket (and its
    // messaging token) so the daemon can deliver bus messages through it
    // instead of the terminal.
    let session_start = serde_json::json!([{
        "hooks": [{
            "type": "command",
            "command": format!(
                "curl -fsS -m 3 -X POST http://127.0.0.1:{daemon_port}/hook \
                 -H \"Authorization: Bearer ${{{bot_token_env}}}\" \
                 -H 'Content-Type: application/json' \
                 -d \"{{\\\"event\\\":\\\"SessionStart\\\",\\\"socket\\\":\\\"$CLAUDE_CODE_MESSAGING_SOCKET\\\",\\\"msg_token\\\":\\\"$CLAUDE_CODE_MESSAGING_TOKEN\\\"}}\" \
                 >/dev/null 2>&1 || true"
            )
        }]
    }]);
    serde_json::json!({
        // Bus deliveries arrive over the cross-session inbox socket; accept
        // them unattended so bot-to-bot traffic flows without approval stops.
        "crossSessionInbound": "accept",
        // Artifacts access is granted at spawn time (`artifacts_allow_rules`
        // via `--allowedTools`), never here: allow rules in a folder's
        // settings.json make Claude Code's trust dialog warn about
        // pre-approved permissions.
        "permissions": {
            "allow": [],
            "deny": [
                "Read(../**)",
                "Read(~/.gravity/secrets/**)",
                "Bash(rm -rf /*)"
            ]
        },
        "hooks": {
            "SessionStart": session_start,
            "UserPromptSubmit": hook_cmd("UserPromptSubmit"),
            // A tool that has finished running is proof the session is
            // executing again: nothing else reports that a pending permission
            // prompt was answered.
            "PostToolUse": hook_cmd("PostToolUse"),
            "Stop": hook_cmd("Stop"),
            "Notification": notification,
            "SessionEnd": hook_cmd("SessionEnd")
        }
    })
}

pub fn atomic_write_json(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_string_pretty(value)?;
    fs::write(&tmp, data).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(())
}

pub(super) fn atomic_write_private_json(
    path: &Path,
    value: &serde_json::Value,
) -> anyhow::Result<()> {
    let tmp = path.with_extension(format!("json.tmp-{}", uuid::Uuid::new_v4()));
    let result = (|| -> anyhow::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .with_context(|| format!("creating {}", tmp.display()))?;
        serde_json::to_writer_pretty(&mut file, value)
            .with_context(|| format!("writing {}", tmp.display()))?;
        file.write_all(b"\n")
            .with_context(|| format!("writing {}", tmp.display()))?;
        file.sync_all()
            .with_context(|| format!("syncing {}", tmp.display()))?;
        fs::rename(&tmp, path)
            .with_context(|| format!("renaming {} to {}", tmp.display(), path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
