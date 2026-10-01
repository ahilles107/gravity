//! Files that travel with a result. `complete_task` lists artifact paths in
//! the `done` body; a path on the other machine is useless to the requester,
//! so the files themselves cross the link and the listing is rewritten to
//! where they landed.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use bus::{Bot, Peer, MAX_PEER_ARTIFACTS_TOTAL_BYTES, MAX_PEER_ARTIFACT_BYTES};

use crate::app::AppState;

use super::frames::{ArtifactFrame, MessageFrame};

/// The separator `complete_task` writes between a result and its listing.
const LISTING: &str = "\n\nartifacts:";

/// Splits a result body into its text and the artifact paths it lists.
pub(super) fn split_listing(body: &str) -> (&str, Vec<&str>) {
    match body.rsplit_once(LISTING) {
        Some((result, listing)) => {
            let paths = listing
                .lines()
                .filter_map(|line| line.strip_prefix("- "))
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .collect();
            (result, paths)
        }
        None => (body, Vec::new()),
    }
}

/// Reads the files a result lists, for sending with it. Only files inside the
/// sending bot's own directory or its project's artifacts directory are read,
/// so a bot cannot exfiltrate anything else by listing it.
pub(super) fn collect(app: &Arc<AppState>, sender: &Bot, body: &str) -> Vec<ArtifactFrame> {
    let (_, paths) = split_listing(body);
    let roots = allowed_roots(app, sender);
    let workspace = PathBuf::from(&sender.workspace_path);
    let mut total = 0u64;
    paths
        .into_iter()
        .map(|listed| {
            let skipped = |reason: &str| ArtifactFrame {
                path: listed.to_string(),
                bytes: None,
                skipped: Some(reason.to_string()),
            };
            let path = workspace.join(listed);
            let Ok(path) = path.canonicalize() else {
                return skipped("file not found");
            };
            if !roots.iter().any(|root| path.starts_with(root)) {
                return skipped("outside the bot's workspace and the project artifacts");
            }
            let size = match path.metadata() {
                Ok(meta) if meta.is_file() => meta.len(),
                _ => return skipped("not a regular file"),
            };
            if size > MAX_PEER_ARTIFACT_BYTES {
                return skipped("larger than the 16 MiB transfer limit");
            }
            if total + size > MAX_PEER_ARTIFACTS_TOTAL_BYTES {
                return skipped("over the 48 MiB limit for one result");
            }
            match std::fs::read(&path) {
                Ok(bytes) => {
                    total += size;
                    ArtifactFrame {
                        path: listed.to_string(),
                        bytes: Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
                        skipped: None,
                    }
                }
                Err(_) => skipped("could not be read"),
            }
        })
        .collect()
}

fn allowed_roots(app: &Arc<AppState>, sender: &Bot) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(bot_root) = Path::new(&sender.workspace_path).parent() {
        roots.extend(bot_root.canonicalize().ok());
    }
    if let Ok(Some(project)) = app.db.get_project(&sender.project_id) {
        let artifacts = crate::paths::artifacts_dir(&app.cfg, &project.dir_name);
        roots.extend(artifacts.canonicalize().ok());
    }
    roots
}

/// Writes a forwarded result's files into the recipient project's artifacts
/// directory and returns the body with its listing pointing at them.
pub(super) fn store(
    app: &Arc<AppState>,
    peer: &Peer,
    to: &Bot,
    frame: &MessageFrame,
) -> anyhow::Result<String> {
    let project = app
        .db
        .get_project(&to.project_id)?
        .ok_or_else(|| anyhow::anyhow!("project missing"))?;
    let short: String = frame.id.chars().take(8).collect();
    let dir = crate::paths::artifacts_dir(&app.cfg, &project.dir_name)
        .join("peers")
        .join(bus::names::dir_name(&peer.name))
        .join(short);
    let (result, _) = split_listing(&frame.body);
    let mut listing = String::new();
    for artifact in &frame.artifacts {
        let line = match (&artifact.bytes, &artifact.skipped) {
            (Some(encoded), _) => {
                let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
                std::fs::create_dir_all(&dir)?;
                let target = unique_path(&dir, &file_name(&artifact.path));
                std::fs::write(&target, bytes)?;
                target.display().to_string()
            }
            (None, reason) => format!(
                "{} (on {}; not transferred: {})",
                artifact.path,
                peer.name,
                reason.as_deref().unwrap_or("unknown reason")
            ),
        };
        listing.push_str(&format!("\n- {line}"));
    }
    Ok(format!("{result}{LISTING}{listing}"))
}

/// The last component of a path written on either platform.
fn file_name(path: &str) -> String {
    let base = path.rsplit(['/', '\\']).next().unwrap_or(path).trim();
    if base.is_empty() || base == "." || base == ".." {
        "artifact".to_string()
    } else {
        base.to_string()
    }
}

/// `dir/name`, or `dir/name-2` and so on when two artifacts share a name.
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (2..)
        .map(|n| dir.join(format!("{stem}-{n}{ext}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_the_listing_complete_task_writes() {
        let (result, paths) = split_listing("built it\n\nartifacts:\n- out/app.exe\n- log.txt");
        assert_eq!(result, "built it");
        assert_eq!(paths, vec!["out/app.exe", "log.txt"]);
    }

    #[test]
    fn a_result_without_a_listing_has_no_paths() {
        assert_eq!(split_listing("done"), ("done", Vec::new()));
    }

    #[test]
    fn takes_the_file_name_from_windows_and_unix_paths() {
        assert_eq!(file_name(r"C:\work\out\app.exe"), "app.exe");
        assert_eq!(file_name("/tmp/build/log.txt"), "log.txt");
        assert_eq!(file_name("../.."), "artifact");
    }
}
