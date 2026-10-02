//! Starting a bot's browser: where Node and Chrome are, and the Playwright MCP
//! server each bot's session runs against its own profile.
//!
//! The daemon runs under launchd or as a Windows service, whose PATH rarely
//! includes a Node installed through a version manager, so Node is looked up
//! here and handed to the session by absolute path.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::Config;

/// The MCP server's name in a bot's session. Its tools reach the bot as
/// `mcp__playwright__browser_*`.
pub const SERVER: &str = "playwright";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserConfig {
    /// Give every bot a browser of its own.
    pub enabled: bool,
    /// The directory holding `node` and `npx`. Found automatically when unset.
    pub node_dir: Option<PathBuf>,
    /// `chrome`, `msedge` or `chromium`. Found automatically when unset.
    pub channel: Option<String>,
    /// Run without a window. The app shows the browser live either way.
    pub headless: bool,
    /// The npm package that serves the browser tools, pinned so every bot runs
    /// the version this daemon was tested with.
    pub package: String,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            node_dir: None,
            channel: None,
            headless: true,
            package: "@playwright/mcp@0.0.83".to_string(),
        }
    }
}

/// Where a bot's browser keeps its state, under the bot's own directory.
pub struct BotBrowser {
    pub dir: PathBuf,
}

impl BotBrowser {
    pub fn new(bot_root: &Path) -> Self {
        Self {
            dir: bot_root.join("browser"),
        }
    }

    /// The Chrome profile: cookies and logins persist across sessions, and
    /// are the bot's alone.
    pub fn profile(&self) -> PathBuf {
        self.dir.join("profile")
    }

    fn config(&self) -> PathBuf {
        self.dir.join("playwright.json")
    }

    fn output(&self) -> PathBuf {
        self.dir.join("output")
    }
}

/// The MCP server entry for a bot's browser, as `{command, args, env}`, after
/// writing its Playwright config. `None` when browsers are off or Node is not
/// installed, so the session simply starts without one.
pub fn server(cfg: &Config, bot_root: &Path) -> Option<Value> {
    if !cfg.browser.enabled {
        return None;
    }
    let Some(node_dir) = node_dir(cfg) else {
        tracing::warn!("no Node.js found; bots start without a browser of their own");
        return None;
    };
    let browser = BotBrowser::new(bot_root);
    let config = json!({
        "browser": {
            "browserName": "chromium",
            "userDataDir": browser.profile(),
            "launchOptions": {
                "channel": channel(cfg),
                "headless": cfg.browser.headless,
                // A port of Chrome's choosing, written to the profile's
                // DevToolsActivePort file, is how the daemon finds the browser
                // to show it live.
                "args": ["--remote-debugging-port=0"]
            }
        },
        "capabilities": ["vision"],
        "outputDir": browser.output()
    });
    if let Err(e) = std::fs::create_dir_all(&browser.dir)
        .map_err(anyhow::Error::from)
        .and_then(|()| crate::paths::atomic_write_json(&browser.config(), &config))
    {
        tracing::warn!(error = %e, "browser config not written; starting without a browser");
        return None;
    }
    let npx = node_dir.join(if cfg!(windows) { "npx.cmd" } else { "npx" });
    let path = std::env::join_paths(
        std::iter::once(node_dir.clone()).chain(
            std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
                .unwrap_or_default(),
        ),
    )
    .ok()?;
    let args = vec![
        "-y".to_string(),
        cfg.browser.package.clone(),
        "--config".to_string(),
        browser.config().display().to_string(),
    ];
    let (command, args) = launch(&npx.display().to_string(), args, cfg!(windows));
    Some(json!({
        "command": command,
        "args": args,
        "env": { "PATH": path.to_string_lossy() }
    }))
}

/// How a session runs `npx`. On Windows it is a batch script, which neither
/// Claude Code nor Codex can start directly, so it goes through `cmd /c` (the
/// form Claude Code documents for `npx` servers on native Windows).
fn launch(npx: &str, args: Vec<String>, windows: bool) -> (String, Vec<String>) {
    if windows {
        let mut wrapped = vec!["/c".to_string(), npx.to_string()];
        wrapped.extend(args);
        ("cmd".to_string(), wrapped)
    } else {
        (npx.to_string(), args)
    }
}

/// The directory holding `npx`: the configured one, else the first found on
/// PATH, else a version manager's or a standard install.
pub fn node_dir(cfg: &Config) -> Option<PathBuf> {
    let npx = if cfg!(windows) { "npx.cmd" } else { "npx" };
    if let Some(dir) = &cfg.browser.node_dir {
        return Some(dir.clone());
    }
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    on_path
        .into_iter()
        .chain(installs(&cfg.user_home))
        .find(|dir| dir.join(npx).is_file())
}

/// Where Node is commonly installed, newest version first within a manager.
fn installs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let versions = |root: PathBuf, suffix: &str| -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = std::fs::read_dir(&root)
            .map(|entries| entries.flatten().map(|e| e.path().join(suffix)).collect())
            .unwrap_or_default();
        found.sort();
        found.reverse();
        found
    };
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(var) {
                dirs.push(PathBuf::from(&base).join("nodejs"));
                dirs.push(PathBuf::from(base).join("Programs").join("nodejs"));
            }
        }
        if let Some(link) = std::env::var_os("NVM_SYMLINK") {
            dirs.push(PathBuf::from(link));
        }
        dirs.push(home.join("scoop/apps/nodejs/current"));
        dirs.extend(versions(
            home.join("AppData/Roaming/fnm/node-versions"),
            "installation",
        ));
    } else {
        dirs.extend(versions(
            home.join(".local/share/mise/installs/node"),
            "bin",
        ));
        dirs.extend(versions(home.join(".nvm/versions/node"), "bin"));
        dirs.extend(versions(home.join(".asdf/installs/nodejs"), "bin"));
        dirs.extend(versions(
            home.join(".local/share/fnm/node-versions"),
            "installation/bin",
        ));
        dirs.push(home.join(".volta/bin"));
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    dirs
}

/// The installed browser to drive: Chrome when present, else Edge, else
/// Playwright's own Chromium.
fn channel(cfg: &Config) -> String {
    if let Some(channel) = &cfg.browser.channel {
        return channel.clone();
    }
    let present = |paths: &[PathBuf]| paths.iter().any(|p| p.exists());
    let (chrome, edge): (Vec<PathBuf>, Vec<PathBuf>) = if cfg!(target_os = "macos") {
        (
            vec!["/Applications/Google Chrome.app".into()],
            vec!["/Applications/Microsoft Edge.app".into()],
        )
    } else if cfg!(windows) {
        let under = |suffix: &str| -> Vec<PathBuf> {
            ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
                .iter()
                .filter_map(std::env::var_os)
                .map(|base| PathBuf::from(base).join(suffix))
                .collect()
        };
        (
            under("Google/Chrome/Application/chrome.exe"),
            under("Microsoft/Edge/Application/msedge.exe"),
        )
    } else {
        (
            vec![
                "/usr/bin/google-chrome".into(),
                "/opt/google/chrome/chrome".into(),
            ],
            vec!["/usr/bin/microsoft-edge".into()],
        )
    };
    if present(&chrome) {
        "chrome".to_string()
    } else if present(&edge) {
        "msedge".to_string()
    } else {
        "chromium".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configured_node_dir_wins_and_writes_the_profile_config() {
        let tmp = tempfile::tempdir().expect("tmp");
        let node = tmp.path().join("node");
        let mut cfg = Config {
            home: tmp.path().to_path_buf(),
            ..Config::default()
        };
        cfg.browser.node_dir = Some(node.clone());
        cfg.browser.channel = Some("msedge".to_string());
        let root = tmp.path().join("bot");
        let entry = server(&cfg, &root).expect("server");
        let npx = entry["command"].as_str().expect("command");
        assert!(npx.starts_with(node.to_str().expect("utf8")), "{npx}");
        assert_eq!(entry["args"][1], cfg.browser.package);
        let written: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("browser/playwright.json")).expect("config"),
        )
        .expect("json");
        assert_eq!(written["browser"]["launchOptions"]["channel"], "msedge");
        assert_eq!(written["browser"]["launchOptions"]["headless"], true);
        assert_eq!(
            written["browser"]["userDataDir"],
            root.join("browser/profile").to_str().expect("utf8")
        );
        assert_eq!(written["capabilities"], json!(["vision"]));
    }

    #[test]
    fn windows_runs_npx_through_cmd() {
        let args = vec!["-y".to_string(), "@playwright/mcp@0.0.83".to_string()];
        let (command, wrapped) = launch(r"C:\nodejs\npx.cmd", args.clone(), true);
        assert_eq!(command, "cmd");
        assert_eq!(
            wrapped[..2],
            [r"/c".to_string(), r"C:\nodejs\npx.cmd".to_string()]
        );
        assert_eq!(wrapped[2..], args[..]);
        let (command, plain) = launch("/opt/node/bin/npx", args.clone(), false);
        assert_eq!((command.as_str(), plain), ("/opt/node/bin/npx", args));
    }

    #[test]
    fn browsers_can_be_turned_off() {
        let tmp = tempfile::tempdir().expect("tmp");
        let mut cfg = Config::default();
        cfg.browser.enabled = false;
        assert!(server(&cfg, tmp.path()).is_none());
    }
}
