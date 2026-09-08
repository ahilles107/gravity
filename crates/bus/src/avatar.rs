//! Avatar values: a short validated string, never an image reference.
//!
//! Two forms are accepted — `icon:<name>`, naming one of the built-in bot
//! icons the client ships, and `color:#rrggbb`. File paths, `data:` URIs and
//! remote URLs are deliberately rejected: a path escapes the workspace
//! sandbox, and the other two would let a bot push arbitrary bytes into the
//! client's renderer or make it fetch over the network. An empty string means
//! "no avatar"; clients derive a swatch colour from the bot id instead, though
//! in practice every bot is dealt an icon at creation.
//!
//! An icon is named rather than transferred, so the contract stays a short
//! string: the daemon validates the name against [`ICONS`] and the client maps
//! it to a bundled asset. Adding a user-supplied `image:<asset_id>` later needs
//! a daemon-side asset store but no change to this contract.

use std::fmt;

/// Upper bound on the stored string. Generous for the accepted forms and small
/// enough that the value stays cheap to ship in every `bot` frame.
pub const MAX_AVATAR_BYTES: usize = 64;

/// The built-in bot icons, in the order the client shows them in its picker.
///
/// This list is the contract: the daemon rejects any other name, so a client
/// never has to render an icon it does not have. Adding one means shipping the
/// asset and appending here in the same change.
pub const ICONS: [&str; 20] = [
    "orbit", "ember", "moss", "nova", "tide", "quartz", "volt", "dusk", "copper", "frost", "halo",
    "glitch", "slate", "bloom", "echo", "pixel", "rune", "cloud", "comet", "mint",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Avatar {
    /// No avatar set; the client derives one from the bot id.
    None,
    /// One of [`ICONS`].
    Icon(String),
    /// Lowercase `#rrggbb`.
    Color(String),
}

impl Avatar {
    /// The canonical stored form, round-tripping through [`parse`].
    pub fn as_stored(&self) -> String {
        match self {
            Avatar::None => String::new(),
            Avatar::Icon(i) => format!("icon:{i}"),
            Avatar::Color(c) => format!("color:{c}"),
        }
    }
}

impl fmt::Display for Avatar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_stored())
    }
}

/// Validate an avatar value, returning the parsed form.
///
/// The error strings are shown to bots as MCP tool errors, so they name the
/// accepted forms — and, for an icon, the whole list — rather than just
/// rejecting the input.
pub fn parse(raw: &str) -> Result<Avatar, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(Avatar::None);
    }
    if raw.len() > MAX_AVATAR_BYTES {
        return Err(format!(
            "avatar exceeds {MAX_AVATAR_BYTES} bytes; use `icon:<name>` or `color:#rrggbb`"
        ));
    }
    if let Some(icon) = raw.strip_prefix("icon:") {
        return parse_icon(icon);
    }
    if let Some(color) = raw.strip_prefix("color:") {
        return parse_color(color);
    }
    Err(format!(
        "unrecognised avatar '{raw}'; use `icon:<name>` ({}), `color:#rrggbb`, or an empty string",
        ICONS.join(", ")
    ))
}

fn parse_icon(icon: &str) -> Result<Avatar, String> {
    let name = icon.to_ascii_lowercase();
    if !ICONS.contains(&name.as_str()) {
        return Err(format!(
            "unknown icon '{icon}'; pick one of {}",
            ICONS.join(", ")
        ));
    }
    Ok(Avatar::Icon(name))
}

fn parse_color(color: &str) -> Result<Avatar, String> {
    let hex = color
        .strip_prefix('#')
        .ok_or_else(|| format!("colour avatar '{color}' must start with '#'"))?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "colour avatar '{color}' must be six hex digits, e.g. color:#4a90d9"
        ));
    }
    Ok(Avatar::Color(format!("#{}", hex.to_ascii_lowercase())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_none() {
        assert_eq!(parse("").expect("empty"), Avatar::None);
        assert_eq!(parse("   ").expect("blank"), Avatar::None);
    }

    #[test]
    fn accepts_every_built_in_icon() {
        for icon in ICONS {
            assert_eq!(
                parse(&format!("icon:{icon}")).expect("icon"),
                Avatar::Icon(icon.to_string())
            );
        }
    }

    #[test]
    fn normalises_icon_case() {
        assert_eq!(
            parse("icon:Orbit").expect("icon"),
            Avatar::Icon("orbit".to_string())
        );
    }

    /// The client only ships the listed assets, so anything else would render
    /// as a hole.
    #[test]
    fn rejects_unknown_icons() {
        assert!(parse("icon:banana").is_err());
        assert!(parse("icon:").is_err());
        assert!(parse("icon:../../etc/passwd").is_err());
    }

    /// Emoji avatars were the previous form; they are no longer accepted.
    #[test]
    fn rejects_emoji() {
        assert!(parse("emoji:🛠️").is_err());
        assert!(parse("🛠️").is_err());
    }

    #[test]
    fn normalises_colour_case() {
        assert_eq!(
            parse("color:#4A90D9").expect("colour"),
            Avatar::Color("#4a90d9".to_string())
        );
    }

    #[test]
    fn rejects_malformed_colour() {
        assert!(parse("color:4a90d9").is_err());
        assert!(parse("color:#abc").is_err());
        assert!(parse("color:#gggggg").is_err());
    }

    #[test]
    fn rejects_paths_data_uris_and_urls() {
        assert!(parse("../../etc/passwd").is_err());
        assert!(parse("data:image/png;base64,iVBORw0KGgo=").is_err());
        assert!(parse("https://example.com/a.png").is_err());
    }

    #[test]
    fn stored_form_round_trips() {
        for raw in ["", "icon:orbit", "color:#4a90d9"] {
            let parsed = parse(raw).expect("parse");
            let restored = parse(&parsed.as_stored()).expect("reparse");
            assert_eq!(parsed, restored);
        }
    }
}
