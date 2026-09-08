//! Bot name validation and the mapping from a name to its on-disk directory.
//!
//! A bot name is an address: bots reach each other with `send_message(to:
//! "name")`. It is also the seed for the bot's directory
//! under `~/.gravity/projects/<project>/bots/`. Those two roles pull in
//! different directions — the address wants to be human-readable, the
//! directory wants to be filesystem-safe — so the name is validated for the
//! first and *sanitized* for the second.
//!
//! Sanitizing is lossy: `"Bot A"` and `"Bot-A"` both become `bot-a`. The
//! charset below removes the worst of it (no `/`, no `.`), but it cannot
//! remove all of it, so callers must still check the sanitized form against
//! existing directories. See `Db::dir_name_taken`.

pub const MAX_NAME_CHARS: usize = 48;

/// Names that would be ambiguous as message addresses. `all` and `everyone`
/// read as broadcasts, and the rest collide with sender kinds in rendered
/// envelopes (`[msg #3 from user]`).
pub const RESERVED_NAMES: &[&str] = &["all", "everyone", "user", "system", "me", "bot", "routine"];

/// Validate a bot name, returning it trimmed.
///
/// Uniqueness and directory collisions need the database and are checked by
/// the caller; this covers only what the string itself can tell us.
pub fn validate(raw: &str) -> Result<String, String> {
    let name = validate_project(raw)?;
    if RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(&name)) {
        return Err(format!(
            "'{name}' is reserved; it would be ambiguous as a message address"
        ));
    }
    Ok(name)
}

/// Validate a project name, returning it trimmed.
///
/// The same string rules as a bot name minus the reserved list: a project is
/// never a message address, so nothing about it can be ambiguous as one. The
/// charset still matters because the name seeds the project's directory.
pub fn validate_project(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("name is empty".to_string());
    }
    let chars = name.chars().count();
    if chars > MAX_NAME_CHARS {
        return Err(format!("name is {chars} characters (max {MAX_NAME_CHARS})"));
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == ' ' || *c == '-' || *c == '_'))
    {
        return Err(format!(
            "name contains '{bad}'; use letters, digits, spaces, '-' or '_'"
        ));
    }
    // A leading or trailing separator survives into the address but vanishes
    // from most renderings, which makes two bots look identically named.
    if name.starts_with(['-', '_']) || name.ends_with(['-', '_']) {
        return Err("name must not start or end with '-' or '_'".to_string());
    }
    Ok(name.to_string())
}

/// The directory name for a bot, derived from its name.
///
/// Lossy by design — see the module docs. Guaranteed non-empty for any name
/// that passed [`validate`].
pub fn dir_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_names() {
        assert_eq!(validate("  Reviewer  ").expect("trimmed"), "Reviewer");
        assert!(validate("build-bot_2").is_ok());
    }

    #[test]
    fn rejects_empty_overlong_and_odd_charsets() {
        assert!(validate("").is_err());
        assert!(validate("   ").is_err());
        assert!(validate(&"a".repeat(MAX_NAME_CHARS + 1)).is_err());
        assert!(validate("bot/../escape").is_err());
        assert!(validate("bot.name").is_err());
        assert!(validate("emoji🛠️name").is_err());
    }

    #[test]
    fn rejects_edge_separators() {
        assert!(validate("-lead").is_err());
        assert!(validate("lead_").is_err());
    }

    #[test]
    fn rejects_reserved_addresses() {
        assert!(validate("all").is_err());
        assert!(validate("Everyone").is_err());
        assert!(validate("USER").is_err());
    }

    /// A project is not an address, so the reserved list does not apply to it
    /// — but the charset that feeds its directory name still does.
    #[test]
    fn project_names_share_the_charset_but_not_the_reserved_list() {
        assert_eq!(validate_project("  Everyone  ").expect("valid"), "Everyone");
        assert!(validate_project("").is_err());
        assert!(validate_project("proj/../escape").is_err());
        assert!(validate_project(&"a".repeat(MAX_NAME_CHARS + 1)).is_err());
    }

    /// The collision the daemon must still check for in the database: two
    /// valid, distinct names sanitizing to one directory.
    #[test]
    fn sanitizing_is_lossy_across_valid_names() {
        assert_eq!(dir_name("Bot A"), "bot-a");
        assert_eq!(dir_name("Bot-A"), "bot-a");
        assert_eq!(dir_name("bot a"), dir_name("BOT-A"));
    }

    #[test]
    fn dir_name_is_non_empty_for_valid_names() {
        for name in ["a", "Reviewer", "build bot 2"] {
            let validated = validate(name).expect("valid");
            assert!(!dir_name(&validated).is_empty());
        }
    }
}
