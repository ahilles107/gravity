//! Single-instance lock and published runtime port for a daemon home.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;

/// Exclusive claim on a daemon home, released when this process exits.
pub struct HomeLock {
    _file: File,
}

/// Claims `home` for this process.
///
/// Two daemons sharing a home run two supervisors against one database and
/// start every bot twice. A port collision used to stop the second one; port
/// negotiation means it no longer does, so the claim is explicit.
pub fn lock(home: &Path) -> anyhow::Result<HomeLock> {
    std::fs::create_dir_all(home)?;
    let path = home.join("gravityd.lock");
    let file = File::create(&path).with_context(|| format!("creating {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(HomeLock { _file: file }),
        Err(std::fs::TryLockError::WouldBlock) => anyhow::bail!(
            "another gravityd is already running against {}; stop it first",
            home.display()
        ),
        Err(std::fs::TryLockError::Error(e)) => {
            Err(e).with_context(|| format!("locking {}", path.display()))
        }
    }
}

pub fn runtime_port_path(home: &Path) -> PathBuf {
    home.join("gravityd.port")
}

/// Publishes the port selected for this process without changing gravityd.toml.
pub fn publish_runtime_port(home: &Path, port: u16) -> anyhow::Result<()> {
    std::fs::create_dir_all(home)?;
    let path = runtime_port_path(home);
    let staged = home.join(format!("gravityd.port.{}.tmp", std::process::id()));
    std::fs::write(&staged, format!("{port}\n"))?;
    std::fs::rename(staged, path)?;
    Ok(())
}

/// Withdraws the published port on a clean stop, so nothing sends a client to a
/// port this machine no longer serves. A crash leaves the file behind, which is
/// why every reader verifies the port with `/health` before trusting it.
pub fn clear_runtime_port(home: &Path) {
    if let Err(e) = std::fs::remove_file(runtime_port_path(home)) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(error = %e, "could not remove the published port");
        }
    }
}

/// How many restarts-to-reclaim are tolerated before the daemon settles for the
/// fallback port, and the window they are counted over.
pub const MAX_RECLAIMS: u32 = 3;
const RECLAIM_WINDOW: Duration = Duration::from_secs(3600);

fn reclaims_path(home: &Path) -> PathBuf {
    home.join("gravityd.reclaims")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Reads `<count> <first restart, unix seconds>`, forgetting a run that has
/// aged out of the window.
fn read_reclaims(home: &Path) -> Option<(u32, u64)> {
    let text = std::fs::read_to_string(reclaims_path(home)).ok()?;
    let (count, started) = text.trim().split_once(' ')?;
    let count = count.parse::<u32>().ok()?;
    let started = started.parse::<u64>().ok()?;
    if now_secs().saturating_sub(started) > RECLAIM_WINDOW.as_secs() {
        return None;
    }
    Some((count, started))
}

/// Restarts already spent taking the configured port back, within the window.
pub fn recent_reclaims(home: &Path) -> u32 {
    read_reclaims(home).map_or(0, |(count, _)| count)
}

/// Counts one restart-to-reclaim, returning the new total for the window.
///
/// A squatter that takes the port, releases it, and takes it again would
/// otherwise bounce the daemon — and every bot with it — indefinitely.
pub fn record_reclaim(home: &Path) -> u32 {
    let (count, started) = read_reclaims(home).unwrap_or((0, now_secs()));
    let count = count + 1;
    if let Err(e) = std::fs::write(reclaims_path(home), format!("{count} {started}\n")) {
        tracing::warn!(error = %e, "could not record the reclaim attempt");
    }
    count
}

/// Forgets the count, called once the daemon holds its configured port again.
pub fn clear_reclaims(home: &Path) {
    if let Err(e) = std::fs::remove_file(reclaims_path(home)) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(error = %e, "could not clear the reclaim count");
        }
    }
}

/// The port a daemon last published for this home, if any.
pub fn runtime_port(home: &Path) -> Option<u16> {
    std::fs::read_to_string(runtime_port_path(home))
        .ok()
        .and_then(|text| text.trim().parse::<u16>().ok())
        .filter(|port| *port > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_the_runtime_port_atomically() {
        let home = tempfile::tempdir().expect("temporary home");

        publish_runtime_port(home.path(), 50_123).expect("publish port");

        assert_eq!(
            std::fs::read_to_string(runtime_port_path(home.path())).expect("runtime port"),
            "50123\n"
        );
        assert_eq!(runtime_port(home.path()), Some(50_123));
    }

    #[test]
    fn clearing_the_runtime_port_leaves_nothing_to_read() {
        let home = tempfile::tempdir().expect("temporary home");
        publish_runtime_port(home.path(), 50_123).expect("publish port");

        clear_runtime_port(home.path());

        assert_eq!(runtime_port(home.path()), None);
        clear_runtime_port(home.path());
    }

    #[test]
    fn reclaim_restarts_accumulate_until_they_are_cleared() {
        let home = tempfile::tempdir().expect("temporary home");

        assert_eq!(recent_reclaims(home.path()), 0);
        assert_eq!(record_reclaim(home.path()), 1);
        assert_eq!(record_reclaim(home.path()), 2);
        assert_eq!(recent_reclaims(home.path()), 2);

        clear_reclaims(home.path());

        assert_eq!(recent_reclaims(home.path()), 0);
    }

    #[test]
    fn reclaim_restarts_outside_the_window_are_forgotten() {
        let home = tempfile::tempdir().expect("temporary home");
        let aged = now_secs() - RECLAIM_WINDOW.as_secs() - 1;
        std::fs::write(
            reclaims_path(home.path()),
            format!("{MAX_RECLAIMS} {aged}\n"),
        )
        .expect("aged count");

        assert_eq!(recent_reclaims(home.path()), 0);
        assert_eq!(record_reclaim(home.path()), 1);
    }

    #[test]
    fn a_second_daemon_cannot_claim_the_same_home() {
        let home = tempfile::tempdir().expect("temporary home");
        let held = lock(home.path()).expect("first lock");

        assert!(lock(home.path()).is_err());

        drop(held);
        lock(home.path()).expect("lock after release");
    }
}
