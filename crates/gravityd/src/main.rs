use std::path::PathBuf;
use std::sync::Arc;

use gravityd::app::AppState;
use gravityd::config::Config;
use gravityd::db::Db;

/// Exit code for a stop that only asks to be restarted, distinct from a crash.
const RECLAIM_EXIT_CODE: i32 = 75;

fn flag_value(args: &[String], flag: &str) -> Option<PathBuf> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--third-party-notices") {
        print!(
            "{}",
            include_str!("../../../third-party/DAEMON_NOTICES.txt")
        );
        return Ok(());
    }
    let config_path = flag_value(&args, "--config");
    let negotiate_port = args.iter().any(|arg| arg == "--negotiate-port");

    // stderr, not stdout: launchd routes the two to different files, and the
    // desktop app reads the error log when an install never comes up. On
    // stdout these lines were interleaved with CLI output and never seen.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let mut cfg = Config::load(config_path.as_deref())?;

    match args.first().map(String::as_str) {
        Some("backup") => {
            let out = flag_value(&args, "--out").unwrap_or_else(|| {
                cfg.home.join("backups").join(format!(
                    "backup-{}",
                    chrono::Utc::now().format("%Y%m%d-%H%M%S")
                ))
            });
            let db = Db::open(&cfg.db_path())?;
            let path = gravityd::backup::backup(&cfg, &db, &out)?;
            println!("backup written to {}", path.display());
            return Ok(());
        }
        Some("restore") => {
            let from = flag_value(&args, "--from")
                .ok_or_else(|| anyhow::anyhow!("restore requires --from <backup dir>"))?;
            let overwrite = args.iter().any(|a| a == "--overwrite");
            gravityd::backup::restore(&cfg, &from, overwrite)?;
            println!("restored into {}", cfg.home.display());
            return Ok(());
        }
        Some("service") => {
            let paths =
                gravityd::service::ServicePaths::new(cfg.home.clone(), cfg.user_home.clone());
            match args.get(1).map(String::as_str) {
                Some("install") => {
                    let source =
                        flag_value(&args, "--binary").map_or_else(std::env::current_exe, Ok)?;
                    gravityd::service::install(&source, &paths)?;
                    gravityd::service::reload(&paths)?;
                    println!(
                        "gravityd installed to {} and running ({})",
                        paths.bin_path().display(),
                        gravityd::service::LAUNCHD_LABEL
                    );
                    println!("Logs: {}", paths.log_dir().display());
                }
                Some("uninstall") => {
                    gravityd::service::uninstall(&paths)?;
                    println!(
                        "gravityd service removed; state in {} kept",
                        cfg.home.display()
                    );
                }
                Some("restart") => {
                    anyhow::ensure!(
                        paths.plist_path().exists(),
                        "no gravityd service installed at {}",
                        paths.plist_path().display()
                    );
                    gravityd::service::reload(&paths)?;
                    println!("gravityd restarted ({})", gravityd::service::LAUNCHD_LABEL);
                }
                Some("status") => {
                    if !gravityd::service::status(&paths, cfg.port) {
                        std::process::exit(1);
                    }
                }
                _ => anyhow::bail!("usage: gravityd service <install|uninstall|restart|status>"),
            }
            return Ok(());
        }
        _ => {}
    }

    // Before the port: two daemons on one home run two supervisors against one
    // database, and negotiation means a collision no longer stops the second.
    let _home_lock = gravityd::home::lock(&cfg.home)?;
    let policy = gravityd::server::port_policy(&cfg, negotiate_port);
    let bound = gravityd::server::bind(&cfg.bind, cfg.port, policy).await?;
    cfg.port = bound.port();
    if bound.negotiated() {
        tracing::warn!(
            configured_port = cfg.configured_port,
            port = cfg.port,
            "serving on a negotiated port: the bot bus is no longer at the \
             configured /mcp URL, which a policy-managed Mac silently drops"
        );
    }
    let home = cfg.home.clone();
    let configured_port = cfg.configured_port;
    gravityd::home::publish_runtime_port(&home, cfg.port)?;

    // Only a daemon that had to move watches for its port, and only while it
    // has restarts left: a port that keeps flapping is not worth bouncing
    // every bot for.
    let spent = gravityd::home::recent_reclaims(&home);
    let reclaim = bound.negotiated() && spent < gravityd::home::MAX_RECLAIMS;
    if bound.negotiated() {
        if !reclaim {
            tracing::warn!(
                port = configured_port,
                attempts = spent,
                "staying on the negotiated port: too many restarts already spent taking it back"
            );
        }
    } else {
        gravityd::home::clear_reclaims(&home);
    }

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        pid = std::process::id(),
        home = %cfg.home.display(),
        port = cfg.port,
        runtime = ?cfg.runtime,
        "gravityd starting"
    );

    let db = Db::open(&cfg.db_path())?;
    if !db.integrity_check()? {
        anyhow::bail!("SQLite integrity check failed; refusing to start");
    }
    let app: Arc<AppState> = AppState::new(cfg, db)?;

    match app.supervisor.adapter().probe() {
        Ok(v) => tracing::info!(runtime_version = %v, "runtime available"),
        Err(e) => {
            tracing::warn!(error = %e, "runtime probe failed; bots cannot start until resolved")
        }
    }

    let stop = gravityd::server::serve(app, bound, reclaim).await?;
    gravityd::home::clear_runtime_port(&home);
    tracing::info!(pid = std::process::id(), "gravityd stopped");

    if stop == gravityd::server::Stop::ReclaimPort {
        let attempts = gravityd::home::record_reclaim(&home);
        tracing::warn!(
            port = configured_port,
            attempts,
            "exiting so the service restarts on the configured port"
        );
        // Non-zero on purpose: the launchd agent is KeepAlive-on-failure, so a
        // clean exit is the one thing that would not bring the daemon back.
        std::process::exit(RECLAIM_EXIT_CODE);
    }
    Ok(())
}
