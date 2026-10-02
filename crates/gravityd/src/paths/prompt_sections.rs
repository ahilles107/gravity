//! Paragraphs of the system prompt that depend on how a bot is set up: the
//! machines its project spans, and the browsers it may drive.

/// What a bot in a linked project is told about the machines it spans.
pub(super) fn linked(machines: &[String]) -> String {
    if machines.is_empty() {
        return String::new();
    }
    format!(
        "This project is linked with {}: one team across machines, each \
         bot listed on both. `create_bot` with `machine` set to one of them \
         creates the bot there, and you manage it as any bot you created.\n\n",
        machines.join(", ")
    )
}

/// What a bot is told about its own browser, and the owner's Chrome when it
/// may use it.
pub(super) fn browser(own: bool, owners_chrome: bool) -> String {
    if !own {
        return String::new();
    }
    let chrome = if owners_chrome {
        " You may also use the owner's own Chrome (`claude-in-chrome`) when a task \
         needs their logged-in sessions; prefer your own browser otherwise."
    } else {
        ""
    };
    format!(
        "You have a browser of your own: the `playwright` tools (`browser_navigate`, \
         `browser_snapshot`, `browser_click`, `browser_tabs`, and mouse tools that act at \
         coordinates on a screenshot). It is private to you, keeps its logins between \
         sessions, and the owner can watch it from Gravity.{chrome}\n\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_machines_a_linked_project_spans() {
        assert_eq!(linked(&[]), "");
        let text = linked(&["win".to_string()]);
        assert!(text.contains("This project is linked with win"));
        assert!(text.contains("`machine`"));
    }

    #[test]
    fn tells_the_bot_about_its_own_browser() {
        assert_eq!(browser(false, true), "");
        assert!(browser(true, false).contains("browser of your own"));
        assert!(!browser(true, false).contains("claude-in-chrome"));
        assert!(browser(true, true).contains("claude-in-chrome"));
    }
}
