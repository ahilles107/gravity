//! Files a client may read: the project's shared artifacts, and anything
//! under a bot's own directory. Nothing else on the machine is reachable,
//! however the path is spelled.

use std::path::{Path, PathBuf};

use base64::Engine;
use bus::{Bot, Project};
use serde::Serialize;

use crate::app::AppState;

/// Largest file `read_file` returns whole.
const MAX_READ_BYTES: u64 = 16 * 1024 * 1024;
/// How deep and how wide an artifacts listing goes.
const MAX_DEPTH: usize = 4;
const MAX_ENTRIES: usize = 500;

#[derive(Debug, Serialize)]
pub struct Artifact {
    pub path: String,
    /// Relative to the artifacts directory, `/`-separated.
    pub rel: String,
    pub name: String,
    pub size: u64,
    pub modified: Option<String>,
    pub mime: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Who made the file, when that can be told; see `super::authors`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<super::authors::Creator>,
}

#[derive(Debug, Serialize)]
pub struct FileBody {
    pub path: String,
    pub name: String,
    pub mime: &'static str,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
    pub truncated: bool,
}

/// Where a `read_file` path is resolved from and confined to.
pub enum Scope<'a> {
    Project(&'a Project),
    Bot(&'a Bot, &'a Project),
}

pub fn artifacts_dir(app: &AppState, project: &Project) -> PathBuf {
    crate::paths::artifacts_dir(&app.cfg, &project.dir_name)
}

pub fn list_artifacts(app: &AppState, project: &Project) -> Vec<Artifact> {
    let root = artifacts_dir(app, project);
    let mut out = Vec::new();
    walk(&root, &root, 0, &mut out);
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<Artifact>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_ENTRIES {
            return;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            if depth < MAX_DEPTH {
                walk(root, &path, depth + 1, out);
            }
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| name.clone());
        let mime = mime_for(&name);
        out.push(Artifact {
            path: path.display().to_string(),
            rel,
            title: (mime == "text/markdown").then(|| heading(&path)).flatten(),
            created_by: None,
            name,
            size: meta.len(),
            modified: meta
                .modified()
                .ok()
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()),
            mime,
        });
    }
}

/// A markdown file's first `# ` heading.
fn heading(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut head = vec![0u8; 4096];
    let n = std::fs::File::open(path).ok()?.read(&mut head).ok()?;
    String::from_utf8_lossy(&head[..n])
        .lines()
        .find_map(|l| l.strip_prefix("# ").map(|t| t.trim().to_string()))
}

pub fn read_file(app: &AppState, scope: Scope<'_>, path: &str) -> anyhow::Result<FileBody> {
    let (base, roots) = match scope {
        Scope::Project(project) => {
            let artifacts = artifacts_dir(app, project);
            (artifacts.clone(), vec![artifacts])
        }
        Scope::Bot(bot, project) => {
            let workspace = PathBuf::from(&bot.workspace_path);
            let bot_root = workspace
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or(workspace.clone());
            (workspace, vec![bot_root, artifacts_dir(app, project)])
        }
    };
    let resolved = base
        .join(path)
        .canonicalize()
        .map_err(|_| anyhow::anyhow!("file not found: {path}"))?;
    let allowed = roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| resolved.starts_with(root));
    anyhow::ensure!(
        allowed,
        "{path} is outside the bot's directory and the project artifacts"
    );
    let meta = resolved.metadata()?;
    anyhow::ensure!(meta.is_file(), "{path} is not a file");
    let name = resolved
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mime = mime_for(&name);
    let truncated = meta.len() > MAX_READ_BYTES;
    let bytes = read_capped(&resolved, MAX_READ_BYTES)?;
    let (text, base64) = if mime.starts_with("image/") || mime == "application/pdf" {
        (
            None,
            Some(base64::engine::general_purpose::STANDARD.encode(&bytes)),
        )
    } else {
        match String::from_utf8(bytes) {
            Ok(text) => (Some(text), None),
            Err(e) => (
                None,
                Some(base64::engine::general_purpose::STANDARD.encode(e.into_bytes())),
            ),
        }
    };
    Ok(FileBody {
        path: resolved.display().to_string(),
        name,
        mime,
        size: meta.len(),
        text,
        base64,
        truncated,
    })
}

fn read_capped(path: &Path, max: u64) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(max)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("md" | "markdown") => "text/markdown",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("json") => "application/json",
        Some("csv") => "text/csv",
        Some("html" | "htm") => "text/html",
        Some(
            "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "py" | "swift" | "go" | "java" | "kt"
            | "c" | "h" | "cpp" | "cs" | "rb" | "sh" | "ps1" | "toml" | "yaml" | "yml" | "css"
            | "sql" | "xml",
        ) => "text/x-code",
        Some("txt" | "log") | None => "text/plain",
        _ => "application/octet-stream",
    }
}

/// Largest file the owner may attach from the app.
const MAX_UPLOAD_BYTES: u64 = 16 * 1024 * 1024;

/// Where an attachment upload stands after one chunk.
#[derive(Debug, Serialize)]
pub struct Upload {
    pub upload_id: String,
    /// Set once the last chunk is in: where the file landed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Appends one chunk of a file the owner attached in the composer. Chunks
/// collect in a hidden partial file under the project's `artifacts/uploads/`;
/// the last one moves it to its final name. Control frames are small, so a
/// file arrives in pieces; only the name's last component is kept, so an
/// attachment cannot be written anywhere else.
pub fn append_upload(
    app: &AppState,
    project: &Project,
    upload_id: Option<&str>,
    name: &str,
    bytes: &[u8],
    last: bool,
) -> anyhow::Result<Upload> {
    use std::io::Write;
    let upload_id = match upload_id {
        Some(id) => {
            anyhow::ensure!(uuid::Uuid::parse_str(id).is_ok(), "not an upload id");
            id.to_string()
        }
        None => bus::new_id(),
    };
    let dir = artifacts_dir(app, project).join("uploads");
    std::fs::create_dir_all(&dir)?;
    let partial = dir.join(format!(".partial-{upload_id}"));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&partial)?;
    file.write_all(bytes)?;
    if file.metadata()?.len() > MAX_UPLOAD_BYTES {
        let _ = std::fs::remove_file(&partial);
        anyhow::bail!("attachments are limited to 16 MB");
    }
    if !last {
        return Ok(Upload {
            upload_id,
            path: None,
        });
    }
    let path = unique_upload_path(&dir, name);
    std::fs::rename(&partial, &path)?;
    Ok(Upload {
        upload_id,
        path: Some(path.display().to_string()),
    })
}

fn unique_upload_path(dir: &Path, name: &str) -> PathBuf {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .map(str::trim)
        .filter(|n| !n.is_empty() && !n.starts_with('.'))
        .unwrap_or("attachment");
    let (stem, ext) = match base.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
        _ => (base.to_string(), String::new()),
    };
    let mut path = dir.join(base);
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem}-{n}{ext}"));
        n += 1;
    }
    path
}
