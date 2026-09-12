//! Shared daemon state wiring.

use std::sync::Arc;
use std::time::{Instant, SystemTime};

use crate::config::{Config, RuntimeKind};
use crate::db::Db;
use crate::events::Events;
use crate::overrides::{AutoCompactOverride, AUTO_COMPACT_META_KEY};
use crate::runtime::double::DoubleAdapter;
use crate::runtime::pty::PtyAdapter;
use crate::runtime::RuntimeAdapter;
use crate::secrets::Secrets;
use crate::supervisor::Supervisor;

pub const PROTOCOL_VERSION: u32 = 2;
pub const DAEMON_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Features this daemon serves, advertised in `hello_ok`. Informational: the
/// client uses it to hide a panel it cannot fill, while authorisation stays
/// with the connection's grants.
pub const CAPABILITIES: &[&str] = &[
    "terminal_attach",
    "search",
    "routines",
    "devices",
    "bot_self_management",
    "config",
    "decisions",
];

pub struct AppState {
    pub cfg: Config,
    pub db: Db,
    pub events: Events,
    pub secrets: Arc<Secrets>,
    pub supervisor: Supervisor,
    /// Runtime override for `cfg.auto_compact_window`, shared with the
    /// supervisor and persisted in the `meta` table.
    pub auto_compact: AutoCompactOverride,
    pub started_at: Instant,
    /// Wall-clock start, kept alongside the monotonic `started_at` purely so
    /// [`AppState::stale_build`] can compare it against a file mtime.
    started_wall: SystemTime,
}

impl AppState {
    pub fn new(cfg: Config, db: Db) -> anyhow::Result<Arc<Self>> {
        std::fs::create_dir_all(&cfg.home)?;
        std::fs::create_dir_all(cfg.logs_dir())?;
        std::fs::create_dir_all(cfg.projects_dir())?;
        let secrets = Arc::new(Secrets::open(&cfg.secrets_dir())?);
        let events = Events::new();
        let adapter: Arc<dyn RuntimeAdapter> = match cfg.runtime {
            RuntimeKind::Pty => Arc::new(PtyAdapter),
            RuntimeKind::Double => Arc::new(DoubleAdapter),
        };
        let auto_compact = AutoCompactOverride::default();
        if let Some(stored) = db.get_meta(AUTO_COMPACT_META_KEY)? {
            auto_compact.restore(&stored);
        }
        let supervisor = Supervisor::new(
            adapter,
            cfg.clone(),
            db.clone(),
            events.clone(),
            secrets.clone(),
            auto_compact.clone(),
        );
        Ok(Arc::new(Self {
            cfg,
            db,
            events,
            secrets,
            supervisor,
            auto_compact,
            started_at: Instant::now(),
            started_wall: SystemTime::now(),
        }))
    }

    /// True when the daemon binary on disk is newer than the running process.
    ///
    /// This is the rebuild-without-restart trap: the old process keeps serving
    /// the old MCP tool list, so a bot is told it cannot do something the
    /// source says it can, and everyone debugs the wrong layer. Surfacing it
    /// in diagnostics turns a silent capability gap into a visible "restart me".
    pub fn stale_build(&self) -> bool {
        std::env::current_exe()
            .and_then(|exe| exe.metadata()?.modified())
            .is_ok_and(|modified| modified > self.started_wall)
    }
}
