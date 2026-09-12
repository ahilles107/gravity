//! Shape checks on what a caller supplies.
//!
//! Error text here is read by a model as often as by a person, so each one
//! says what to do instead rather than only what was wrong.

use bus::*;

use super::invalid;

pub fn checked_title(title: &str) -> anyhow::Result<String> {
    let title = title.trim();
    if title.is_empty() {
        return Err(invalid(
            "'title' is required: one line saying what is being decided",
        ));
    }
    if title.chars().count() > MAX_DECISION_TITLE_CHARS {
        return Err(invalid(format!(
            "title is longer than {MAX_DECISION_TITLE_CHARS} characters — put the detail in \
             'body' and keep the title to the question itself"
        )));
    }
    Ok(title.to_string())
}

pub fn checked_body(body: &str) -> anyhow::Result<String> {
    let body = body.trim();
    if body.is_empty() {
        return Err(invalid(
            "'body' is required: the context, the evidence and what you will do meanwhile",
        ));
    }
    if body.len() > MAX_DECISION_BODY_BYTES {
        return Err(invalid(format!(
            "body exceeds {MAX_DECISION_BODY_BYTES} bytes — write it to a shared artifact and \
             summarise here with the path"
        )));
    }
    Ok(body.to_string())
}

pub fn checked_comment(body: &str) -> anyhow::Result<String> {
    let body = body.trim();
    if body.is_empty() {
        return Err(invalid("'body' is required"));
    }
    if body.len() > MAX_DECISION_COMMENT_BYTES {
        return Err(invalid(format!(
            "comment exceeds {MAX_DECISION_COMMENT_BYTES} bytes"
        )));
    }
    Ok(body.to_string())
}

/// Options, with unique keys and a cap. A ruling names an option by key, so a
/// duplicate key would make the ruling ambiguous after the fact.
pub fn checked_options(options: &[DecisionOption]) -> anyhow::Result<Vec<DecisionOption>> {
    if options.len() > MAX_DECISION_OPTIONS {
        return Err(invalid(format!(
            "at most {MAX_DECISION_OPTIONS} options — if the choice needs more, it is really \
             two decisions"
        )));
    }
    let mut seen = std::collections::HashSet::new();
    for option in options {
        if option.key.trim().is_empty() || option.label.trim().is_empty() {
            return Err(invalid("every option needs a 'key' and a 'label'"));
        }
        if option.key.chars().count() > MAX_DECISION_OPTION_CHARS
            || option.label.chars().count() > MAX_DECISION_OPTION_CHARS
        {
            return Err(invalid(format!(
                "option '{}' has a key or label longer than {MAX_DECISION_OPTION_CHARS} \
                 characters — what it costs belongs in the option's 'description'",
                option.key
            )));
        }
        // Lowercased, because a ruling naming 'Ship' when the options are
        // 'ship' and 'Ship' is ambiguous after the fact whichever way we
        // compare later.
        if !seen.insert(option.key.to_lowercase()) {
            return Err(invalid(format!(
                "option key '{}' is used twice; keys identify the ruling later",
                option.key
            )));
        }
    }
    Ok(options.to_vec())
}

/// Find an option by key, case-insensitively, returning its stored spelling.
///
/// Keys are deduplicated case-insensitively in [`checked_options`], so a
/// lookup that is case-*sensitive* would reject a key the caller could
/// reasonably believe it offered. Returning the canonical spelling is what
/// keeps the stored recommendation and ruling pointing at the same string.
fn canonical_key<'a>(value: &str, options: &'a [DecisionOption]) -> Option<&'a str> {
    options
        .iter()
        .find(|o| o.key.eq_ignore_ascii_case(value))
        .map(|o| o.key.as_str())
}

/// A recommendation must name an option that exists, when there are options.
pub fn checked_recommendation(
    recommendation: Option<&str>,
    options: &[DecisionOption],
) -> anyhow::Result<Option<String>> {
    let Some(value) = recommendation.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    if options.is_empty() {
        return Ok(Some(value.to_string()));
    }
    let key = canonical_key(value, options).ok_or_else(|| {
        invalid(format!(
            "recommendation '{value}' is not one of the option keys you offered: {}",
            options
                .iter()
                .map(|o| o.key.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })?;
    Ok(Some(key.to_string()))
}

/// A ruling option, likewise, must be one the decision actually offers.
pub fn checked_ruling_option(
    option: Option<&str>,
    options: &[DecisionOption],
) -> anyhow::Result<Option<String>> {
    let Some(value) = option.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let key = canonical_key(value, options)
        .ok_or_else(|| invalid(format!("'{value}' is not an option on this decision")))?;
    Ok(Some(key.to_string()))
}

/// Tag names are `[a-z0-9-]{1,32}`, lowercased on the way in.
pub fn checked_tags(tags: &[String]) -> anyhow::Result<Vec<String>> {
    let mut out = Vec::new();
    for tag in tags {
        let name = tag.trim().to_lowercase();
        if name.is_empty() || name.len() > 32 {
            return Err(invalid(format!(
                "tag '{tag}' must be 1 to 32 characters of a-z, 0-9 and '-'"
            )));
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(invalid(format!(
                "tag '{tag}' must use only a-z, 0-9 and '-' — list_tags shows what this \
                 project already files under"
            )));
        }
        if !out.contains(&name) {
            out.push(name);
        }
    }
    if out.len() > MAX_DECISION_TAGS {
        return Err(invalid(format!(
            "at most {MAX_DECISION_TAGS} tags — filing is for finding a ruling again, and a \
             record under every tag is under none"
        )));
    }
    Ok(out)
}

/// A tag's prose description, which is what stops one bot filing `spend` and
/// another `budget` for the same thing. `None` leaves an existing one alone.
pub fn checked_tag_description(description: Option<&str>) -> anyhow::Result<Option<&str>> {
    let Some(value) = description else {
        return Ok(None);
    };
    if value.chars().count() > MAX_TAG_DESCRIPTION_CHARS {
        return Err(invalid(format!(
            "a tag description is at most {MAX_TAG_DESCRIPTION_CHARS} characters — say what \
             belongs under it, not what is filed there"
        )));
    }
    Ok(Some(value))
}

/// A tag's colour, as `#rgb` or `#rrggbb`.
///
/// Checked rather than stored as given: this reaches the owner's window as a
/// style, and a tag colour is not a place a bot gets to put arbitrary text.
pub fn checked_color(color: Option<&str>) -> anyhow::Result<Option<&str>> {
    let Some(value) = color.map(str::trim).filter(|c| !c.is_empty()) else {
        return Ok(None);
    };
    let digits = value.strip_prefix('#').unwrap_or("");
    if !matches!(digits.len(), 3 | 6) || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid(format!(
            "'{value}' is not a colour — use '#rgb' or '#rrggbb'"
        )));
    }
    Ok(Some(value))
}

pub fn checked_deadline(
    value: Option<&str>,
) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|t| Some(t.with_timezone(&chrono::Utc)))
        .map_err(|_| invalid(format!("'{value}' is not an RFC 3339 timestamp")))
}
