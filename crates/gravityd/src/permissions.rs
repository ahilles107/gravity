//! Private token/config files: Unix modes or a current-user Windows ACL.
use std::path::Path;

#[cfg(unix)]
pub fn private(path: &Path, directory: bool) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if directory { 0o700 } else { 0o600 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(windows)]
pub fn user_sid() -> anyhow::Result<String> {
    use anyhow::Context;
    let out = std::process::Command::new("whoami.exe")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .context("identifying the Windows user")?;
    anyhow::ensure!(out.status.success(), "cannot identify Windows user");
    let text = String::from_utf8(out.stdout)?;
    let sid = text
        .trim()
        .rsplit(',')
        .next()
        .context("missing Windows SID")?
        .trim_matches('"');
    anyhow::ensure!(
        sid.starts_with("S-1-")
            && sid
                .chars()
                .all(|c| c.is_ascii_digit() || c == 'S' || c == '-'),
        "invalid Windows SID"
    );
    Ok(sid.to_string())
}

#[cfg(windows)]
pub fn private(path: &Path, directory: bool) -> anyhow::Result<()> {
    use anyhow::Context;
    let sid = user_sid()?;
    let grant = format!("*{sid}:{}F", if directory { "(OI)(CI)" } else { "" });
    let out = std::process::Command::new("icacls.exe")
        .arg(path)
        .args(["/inheritance:r", "/grant:r", &grant])
        .output()
        .context("restricting private file permissions")?;
    anyhow::ensure!(
        out.status.success(),
        "cannot restrict private file permissions: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}
