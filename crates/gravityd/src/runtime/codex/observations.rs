use serde_json::json;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub(super) fn append(path: &Path, role: &str, text: &str) -> anyhow::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let entry = json!({ "type": role, "timestamp": chrono::Utc::now().to_rfc3339(), "message": { "content": [{ "type": "text", "text": text }] } });
    writeln!(file, "{entry}")?;
    Ok(())
}
