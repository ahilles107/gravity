//! One start at a time per bot. Bot creation starts a new bot right away, and
//! the supervision tick starts any bot without a session; a slow runtime
//! (Codex spins up an app server) leaves both inside `start_bot` together, and
//! two runtimes resuming one conversation crash each other.

use super::*;

/// Proof that this caller owns the bot's start. Dropping it hands the bot back
/// to the next start, whether this one brought a session up or failed.
pub(super) struct StartClaim<'a> {
    sup: &'a Supervisor,
    bot_id: &'a str,
}

impl Supervisor {
    /// Claims the bot for a start, or `None` when it is already running or
    /// another start is in flight.
    pub(super) fn claim_start<'a>(&'a self, bot_id: &'a str) -> Option<StartClaim<'a>> {
        let mut bots = self.lock_bots();
        let handle = bots
            .entry(bot_id.to_string())
            .or_insert_with(|| BotHandle::new(self.inner.cfg.scrollback_bytes));
        if handle.starting || (handle.session.is_some() && handle.state.is_running()) {
            return None;
        }
        handle.starting = true;
        Some(StartClaim { sup: self, bot_id })
    }
}

impl Drop for StartClaim<'_> {
    fn drop(&mut self) {
        if let Some(handle) = self.sup.lock_bots().get_mut(self.bot_id) {
            handle.starting = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::overrides::AutoCompactOverride;
    use crate::runtime::double::DoubleAdapter;
    use crate::runtime::{Capabilities, StartedSession};

    /// A runtime as slow to come up as Codex, counting how often it is asked.
    struct SlowAdapter {
        starts: AtomicUsize,
        fail: bool,
    }

    impl RuntimeAdapter for SlowAdapter {
        fn capabilities(&self) -> Capabilities {
            DoubleAdapter.capabilities()
        }

        fn start(&self, spec: &BotSpec) -> anyhow::Result<StartedSession> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(200));
            if self.fail {
                anyhow::bail!("runtime refused to start");
            }
            DoubleAdapter.start(spec)
        }

        fn probe(&self) -> anyhow::Result<String> {
            DoubleAdapter.probe()
        }
    }

    fn supervisor(home: &std::path::Path, adapter: Arc<SlowAdapter>) -> (Supervisor, String) {
        let cfg = Config {
            home: home.to_path_buf(),
            ..Config::default()
        };
        let db = Db::open(&home.join("bus.sqlite")).expect("db");
        let project = db.create_project("p", "p").expect("project");
        let workspace = home.join("p/bots/alice/workspace");
        let bot = db
            .create_bot(
                &project.id,
                "alice",
                "",
                "",
                "",
                &workspace.display().to_string(),
                "alice",
                None,
            )
            .expect("bot");
        let secrets = Arc::new(Secrets::open(&home.join("secrets")).expect("secrets"));
        let sup = Supervisor::new(
            adapter,
            cfg,
            db,
            Events::new(),
            secrets,
            AutoCompactOverride::default(),
        );
        (sup, bot.id)
    }

    async fn start_concurrently(sup: &Supervisor, bot_id: &str, callers: usize) {
        let tasks: Vec<_> = (0..callers)
            .map(|_| {
                let sup = sup.clone();
                let bot_id = bot_id.to_string();
                tokio::task::spawn_blocking(move || sup.start_bot(&bot_id))
            })
            .collect();
        for task in tasks {
            // A failing runtime fails only the caller that owned the start.
            let _ = task.await.expect("start task");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn concurrent_starts_launch_one_runtime() {
        let home = tempfile::tempdir().expect("tempdir");
        let adapter = Arc::new(SlowAdapter {
            starts: AtomicUsize::new(0),
            fail: false,
        });
        let (sup, bot_id) = supervisor(home.path(), adapter.clone());

        start_concurrently(&sup, &bot_id, 4).await;

        assert_eq!(adapter.starts.load(Ordering::SeqCst), 1);
        assert!(sup.has_session(&bot_id));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_start_releases_the_bot_for_the_next() {
        let home = tempfile::tempdir().expect("tempdir");
        let adapter = Arc::new(SlowAdapter {
            starts: AtomicUsize::new(0),
            fail: true,
        });
        let (sup, bot_id) = supervisor(home.path(), adapter.clone());

        assert!(sup.start_bot(&bot_id).is_err());
        assert!(sup.start_bot(&bot_id).is_err());

        assert_eq!(adapter.starts.load(Ordering::SeqCst), 2);
    }
}
