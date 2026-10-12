//! Auto-approval rules: which permission prompts the daemon answers itself.
//!
//! A rule is a case-insensitive pattern matched against the permission dialog
//! as it appears on the bot's terminal (Claude Code's `Notification` hook only
//! says "Claude needs your permission", without naming the tool). The pattern
//! matches anywhere in the dialog; `*` stands for any run of characters, and
//! dashes (`-`, `–`, `—`) and runs of whitespace are interchangeable. A
//! matching prompt is answered by sending Enter, which takes the dialog's
//! default choice ("Yes"). The list is empty until the user writes one, and is
//! persisted in the `meta` table like the other runtime-writable settings.

use std::sync::{Arc, Mutex};

use crate::db::Db;

/// The `meta` table key the rule list is persisted under (a JSON array).
pub const AUTO_APPROVE_META_KEY: &str = "auto_approve_rules";
/// Upper bounds that keep a runaway client from storing an unbounded list.
pub const MAX_RULES: usize = 100;
pub const MAX_RULE_LEN: usize = 200;

#[derive(Clone, Default)]
pub struct AutoApproveRules {
    rules: Arc<Mutex<Vec<String>>>,
}

impl AutoApproveRules {
    pub fn list(&self) -> Vec<String> {
        self.rules.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn is_empty(&self) -> bool {
        self.rules
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    /// Whether any rule matches `text`.
    pub fn matches(&self, text: &str) -> bool {
        let text = canonical(text);
        let rules = self.rules.lock().unwrap_or_else(|e| e.into_inner());
        rules.iter().any(|rule| pattern_matches(rule, &text))
    }

    /// Validates, persists and publishes a new list as one serialized
    /// operation so concurrent control connections cannot leave the database
    /// and the runtime disagreeing. Returns the normalized list.
    pub fn persist(&self, db: &Db, rules: &[String]) -> anyhow::Result<Vec<String>> {
        let rules = normalize(rules)?;
        let mut current = self.rules.lock().unwrap_or_else(|e| e.into_inner());
        db.set_meta(AUTO_APPROVE_META_KEY, &serde_json::to_string(&rules)?)?;
        *current = rules.clone();
        Ok(rules)
    }

    /// Restores the persisted list; a corrupt row restores nothing, which is
    /// the safe direction for a feature that approves things.
    pub fn restore(&self, stored: &str) {
        if let Ok(rules) = serde_json::from_str::<Vec<String>>(stored) {
            if let Ok(rules) = normalize(&rules) {
                *self.rules.lock().unwrap_or_else(|e| e.into_inner()) = rules;
            }
        }
    }
}

/// Trims rules, drops blanks and duplicates, and enforces the size limits.
/// A rule of only `*` is refused: it would approve every prompt, which is a
/// different decision from listing the prompts you trust.
pub fn normalize(rules: &[String]) -> anyhow::Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for rule in rules {
        let rule = rule.trim();
        if rule.is_empty() {
            continue;
        }
        anyhow::ensure!(
            rule.chars().count() <= MAX_RULE_LEN,
            "a rule can be at most {MAX_RULE_LEN} characters"
        );
        anyhow::ensure!(
            rule.chars().any(|c| c != '*' && !c.is_whitespace()),
            "a rule needs text besides '*'"
        );
        if !out.iter().any(|r| r.eq_ignore_ascii_case(rule)) {
            out.push(rule.to_string());
        }
    }
    anyhow::ensure!(out.len() <= MAX_RULES, "at most {MAX_RULES} rules");
    Ok(out)
}

/// Lowercases, folds en/em dashes to `-` and collapses whitespace runs, so a
/// rule typed with a plain hyphen matches the dialog's "—".
fn canonical(text: &str) -> String {
    let folded: String = text
        .chars()
        .map(|c| {
            if matches!(c, '\u{2013}' | '\u{2014}') {
                '-'
            } else {
                c
            }
        })
        .collect();
    folded
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The most recent permission dialog on a terminal, as plain text: the tail of
/// the raw output with escape sequences removed, cut off just after the last
/// option list ("❯ 1. Yes"), so a dialog that was answered earlier and is
/// still in the scrollback cannot be mistaken for the current one.
pub fn dialog_text(raw: &[u8]) -> String {
    const TAIL_CHARS: usize = 3000;
    let text = strip_escapes(&String::from_utf8_lossy(raw));
    let end = text.rfind('\u{276f}').map_or(text.len(), |i| {
        text[i..].find('\n').map_or(text.len(), |nl| i + nl)
    });
    let upto = &text[..end];
    let skip = upto.chars().count().saturating_sub(TAIL_CHARS);
    upto.chars().skip(skip).collect()
}

/// Removes CSI, OSC and other escape sequences; leaves printable text and
/// newlines.
fn strip_escapes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.next() {
                Some('[') => {
                    for n in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&n) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(n) = chars.next() {
                        if n == '\u{7}' {
                            break;
                        }
                        if n == '\u{1b}' {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\r' => {}
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// Case-insensitive "contains" with `*` wildcards over canonical text.
fn pattern_matches(pattern: &str, text: &str) -> bool {
    let mut rest = text;
    for piece in canonical(pattern).split('*') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        match rest.find(piece) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(list: &[&str]) -> AutoApproveRules {
        let r = AutoApproveRules::default();
        *r.rules.lock().unwrap() = list.iter().map(|s| s.to_string()).collect();
        r
    }

    #[test]
    fn matches_substrings_case_insensitively() {
        let r = rules(&["slack - read channel"]);
        assert!(r.matches("Claude needs your permission to use Slack - Read channel messages"));
        assert!(!r.matches("Claude needs your permission to use Bash"));
    }

    #[test]
    fn star_matches_any_run_in_order() {
        let r = rules(&["use slack*read"]);
        assert!(r.matches("permission to use Slack - Read channel messages"));
        assert!(!r.matches("permission to read Slack, use it"));
    }

    #[test]
    fn hyphen_rules_match_the_dialog_dash() {
        let r = rules(&["Slack - Read channel messages"]);
        assert!(r.matches("Slack \u{2014}  Read channel\nmessages Tool: (MCP)"));
    }

    #[test]
    fn dialog_text_strips_escapes_and_stops_at_the_option_list() {
        let raw = "\u{1b}[1mTool use\u{1b}[0m\r\n Slack \u{2014} Read channel messages\r\n \u{1b}[36m\u{276f} 1. Yes\u{1b}[0m\r\n   2. No\r\n";
        let text = dialog_text(raw.as_bytes());
        assert!(text.contains("Slack \u{2014} Read channel messages"));
        assert!(text.contains("1. Yes"));
        assert!(!text.contains("2. No"));
        assert!(!text.contains('\u{1b}'));
    }

    #[test]
    fn empty_list_approves_nothing() {
        assert!(!AutoApproveRules::default().matches("anything"));
    }

    #[test]
    fn normalize_cleans_and_refuses_match_everything() {
        let cleaned = normalize(&[" Slack ".into(), "".into(), "slack".into()]).unwrap();
        assert_eq!(cleaned, vec!["Slack".to_string()]);
        assert!(normalize(&["*".into()]).is_err());
        assert!(normalize(&[" ** ".into()]).is_err());
        assert!(normalize(&["x".repeat(MAX_RULE_LEN + 1)]).is_err());
    }

    #[test]
    fn restore_ignores_garbage() {
        let r = AutoApproveRules::default();
        r.restore("not json");
        assert!(r.list().is_empty());
        r.restore(r#"["Slack"]"#);
        assert_eq!(r.list(), vec!["Slack".to_string()]);
    }
}
