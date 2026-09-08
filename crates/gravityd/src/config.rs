use std::net::IpAddr;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// Bounds Claude Code accepts for the auto-compact window.
pub(crate) const AUTO_COMPACT_WINDOW_MIN: u32 = 100_000;
pub(crate) const AUTO_COMPACT_WINDOW_MAX: u32 = 1_000_000;
/// Comfortably above the 200k window a non-1M model assumes, so a bot chat
/// keeps far more history before it compacts, while still capping what any
/// single turn re-sends well below the 1M model limit.
pub(crate) const DEFAULT_AUTO_COMPACT_WINDOW: u32 = 250_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Root of all daemon state (`~/.gravity`).
    pub home: PathBuf,
    /// Addresses to bind. Localhost plus, optionally, a Tailscale interface.
    pub bind: Vec<IpAddr>,
    /// The port actually being served, and the one bots are pointed at.
    /// Defaults to 49777 because Atlassian's managed Claude Code policy
    /// allowlists only that loopback `/mcp` URL; a bot session on a
    /// policy-managed Mac silently drops any MCP server whose exact URL is not
    /// allowlisted. Set from `configured_port` unless a fallback was
    /// negotiated at startup.
    pub port: u16,
    /// The port `gravityd.toml` asked for, kept so a negotiated fallback can be
    /// reported as the compromise it is rather than passed off as the setting.
    #[serde(skip)]
    pub configured_port: u16,
    /// Whether the app-managed service may fall back to another port when the
    /// configured one is occupied, restarting onto the configured one once it
    /// comes free. Turning this off trades a temporarily moved port for a
    /// daemon that refuses to start — the right trade on a policy-managed Mac,
    /// where only the allowlisted `/mcp` URL reaches bots at all. Ignored for
    /// direct launches, which always fail on a collision.
    pub negotiate_port: bool,
    /// `pty` (spawn `claude` in a pty) or `double` (deterministic test runtime).
    pub runtime: RuntimeKind,
    pub claude_bin: String,
    /// Extra arguments passed to the `claude` CLI.
    pub claude_args: Vec<String>,
    /// Context window, in tokens, a bot session may grow to before Claude Code
    /// auto-compacts it (`CLAUDE_CODE_AUTO_COMPACT_WINDOW`). Bots are
    /// always-on chat sessions, and every turn re-sends the whole transcript,
    /// so the model default — up to 1M on a 1M-context model — makes a long
    /// conversation expensive. `None` leaves the model default in place.
    /// Claude Code accepts 100k–1M and ignores anything outside that range,
    /// so values are clamped rather than dropped.
    pub auto_compact_window: Option<u32>,
    /// Pin Claude Code to its classic renderer, so xterm keeps the scrollback
    /// and the wheel scrolls it (`CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN`,
    /// `CLAUDE_CODE_DISABLE_MOUSE`).
    pub classic_renderer: bool,
    /// Per-project cap on how many bots may exist at once. The only limit on
    /// bot-authored bot creation: since bots cannot delete anything but their
    /// own children, and archived bots free their slot, this bounds the whole
    /// population regardless of how deeply bots nest their teams.
    pub max_bots_per_project: usize,
    pub delivery: DeliveryConfig,
    pub scheduler: SchedulerConfig,
    /// How often the supervisor reconciles live bots into running sessions.
    /// Bots are always-on, so this is the only thing that starts them.
    pub supervision_interval_ms: u64,
    /// Terminal scrollback ring size per bot, in bytes.
    pub scrollback_bytes: usize,
    /// Extra allowed WebSocket Origin values. Requests without an Origin
    /// header (native clients) and tauri/localhost origins are always allowed.
    pub allowed_origins: Vec<String>,
    pub retention: RetentionConfig,
    /// The *user's* home, where Claude Code keeps its `~/.claude/projects`
    /// transcripts. Distinct from `home`, which is the daemon's own state
    /// directory; separate so tests can point it at a fixture tree.
    pub user_home: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RetentionConfig {
    pub enabled: bool,
    pub message_days: i64,
    pub delivery_days: i64,
    pub routine_run_days: i64,
    /// How long emitted signals are kept once no run references them.
    pub signal_days: i64,
    /// How long identity revisions are kept. This is the undo buffer for
    /// changes bots make to themselves, so it outlives ordinary chat.
    pub revision_days: i64,
    /// How long an archived bot's workspace stays on disk before reclamation.
    pub archived_bot_days: i64,
    /// How often the pruning task runs.
    pub interval_hours: u64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            message_days: 180,
            delivery_days: 30,
            routine_run_days: 90,
            signal_days: 30,
            revision_days: 365,
            archived_bot_days: 30,
            interval_hours: 24,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Pty,
    Double,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DeliveryConfig {
    pub poll_interval_ms: u64,
    pub lease_seconds: i64,
    pub max_attempts: i64,
    pub base_backoff_seconds: i64,
}

impl Default for DeliveryConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 1_000,
            lease_seconds: 60,
            max_attempts: 10,
            base_backoff_seconds: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SchedulerConfig {
    pub tick_interval_ms: u64,
    /// Default deadline for one occurrence, when its routine sets none.
    pub lease_seconds: i64,
    /// How far back to schedule missed occurrences on startup.
    pub catch_up_window_minutes: i64,
    /// First retry delay for a failed occurrence; doubled per attempt.
    pub base_backoff_seconds: i64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            tick_interval_ms: 30_000,
            lease_seconds: 300,
            catch_up_window_minutes: 60,
            base_backoff_seconds: 30,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            home: default_home(),
            bind: vec![IpAddr::from([127, 0, 0, 1])],
            port: 49777,
            configured_port: 49777,
            negotiate_port: true,
            runtime: RuntimeKind::Pty,
            claude_bin: "claude".to_string(),
            claude_args: Vec::new(),
            auto_compact_window: Some(DEFAULT_AUTO_COMPACT_WINDOW),
            classic_renderer: true,
            max_bots_per_project: 12,
            delivery: DeliveryConfig::default(),
            scheduler: SchedulerConfig::default(),
            supervision_interval_ms: 5_000,
            scrollback_bytes: 1_048_576,
            allowed_origins: Vec::new(),
            retention: RetentionConfig::default(),
            user_home: dirs_home(),
        }
    }
}

fn default_home() -> PathBuf {
    resolve_home(
        std::env::var_os("GRAVITY_HOME").map(PathBuf::from),
        dirs_home(),
    )
}

fn resolve_home(gravity_home: Option<PathBuf>, user_home: PathBuf) -> PathBuf {
    gravity_home.unwrap_or_else(|| user_home.join(".gravity"))
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

impl Config {
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let path = match path {
            Some(p) => p.to_path_buf(),
            None => default_home().join("gravityd.toml"),
        };
        if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let mut cfg: Config =
                toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
            cfg.configured_port = cfg.port;
            Ok(cfg)
        } else {
            Ok(Config::default())
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.home.join("bus.sqlite")
    }

    pub fn secrets_dir(&self) -> PathBuf {
        self.home.join("secrets")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.home.join("logs")
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.home.join("projects")
    }

    /// The configured auto-compact window, clamped into the range Claude Code
    /// honours. Out-of-range values are ignored by the CLI, which would
    /// silently restore the expensive model default.
    pub fn effective_auto_compact_window(&self) -> Option<u32> {
        self.auto_compact_window
            .map(|w| w.clamp(AUTO_COMPACT_WINDOW_MIN, AUTO_COMPACT_WINDOW_MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gravity_home_uses_the_new_namespace() {
        assert_eq!(
            resolve_home(None, PathBuf::from("/Users/tester")),
            PathBuf::from("/Users/tester/.gravity")
        );
        assert_eq!(
            resolve_home(
                Some(PathBuf::from("/var/lib/gravity")),
                PathBuf::from("/Users/tester")
            ),
            PathBuf::from("/var/lib/gravity")
        );
    }

    #[test]
    fn auto_compact_window_defaults_below_the_model_window() {
        let cfg = Config::default();
        assert_eq!(
            cfg.effective_auto_compact_window(),
            Some(DEFAULT_AUTO_COMPACT_WINDOW)
        );
    }

    #[test]
    fn auto_compact_window_is_clamped_into_the_accepted_range() {
        let low = Config {
            auto_compact_window: Some(1_000),
            ..Config::default()
        };
        let high = Config {
            auto_compact_window: Some(4_000_000),
            ..Config::default()
        };
        assert_eq!(low.effective_auto_compact_window(), Some(100_000));
        assert_eq!(high.effective_auto_compact_window(), Some(1_000_000));
    }

    #[test]
    fn auto_compact_window_can_be_turned_off() {
        let cfg = Config {
            auto_compact_window: None,
            ..Config::default()
        };
        assert_eq!(cfg.effective_auto_compact_window(), None);
    }

    #[test]
    fn classic_renderer_is_on_unless_configured_off() {
        assert!(Config::default().classic_renderer);
        let cfg: Config = toml::from_str("classic_renderer = false").unwrap();
        assert!(!cfg.classic_renderer);
    }
}
