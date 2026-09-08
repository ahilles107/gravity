//! Defaults for a bot created from a bare name.
//!
//! "Create a bot called Steve" is the common case, and it carries no
//! description and no instructions. Demanding them turns a one-line request
//! into an interrogation; storing them empty produces a blank row in
//! `list_bots` and a bot with no idea why it exists. So a bot created without
//! either gets a charter that says exactly that, and tells it how to replace
//! the placeholder with the real thing.

/// The description other bots see in `list_bots`.
pub fn description(supplied: Option<&str>) -> String {
    match supplied.map(str::trim) {
        Some(text) if !text.is_empty() => text.to_string(),
        _ => "General-purpose bot; no charter set yet.".to_string(),
    }
}

/// Standing instructions appended to the new bot's system prompt.
///
/// `creator` is the name of the bot that made this one, or `None` when a user
/// did — it decides who the new bot should ask about its purpose.
pub fn instructions(supplied: Option<&str>, creator: Option<&str>) -> String {
    match supplied.map(str::trim) {
        Some(text) if !text.is_empty() => text.to_string(),
        _ => {
            let who = creator.unwrap_or("the user");
            format!(
                "You were created from a name alone, so you have no charter yet.\n\n\
                 Ask {who} what you are for before starting any long piece of work, \
                 and treat the first task you are sent as your charter in the \
                 meantime. Once you know what you are for, record it with \
                 `update_self(description, instructions)` — that is what makes it \
                 survive your next start."
            )
        }
    }
}

/// Context for the opening prompt sent to a freshly created bot.
pub struct Introduction<'a> {
    /// The bot that created this one, or `None` when a user did.
    pub creator: Option<&'a str>,
    /// Whether a real description or instructions came with the creation.
    pub charted: bool,
    /// Whether this is the first bot the project has ever had.
    pub first: bool,
}

/// The first message a new bot is sent: introduce yourself.
///
/// A bot starts into an empty session, so until someone types at it its window
/// shows nothing and a freshly created bot reads as broken rather than idle.
/// Asking for an introduction turns creation into a first turn — the bot reads
/// its own charter back, and the person who made it can see it working.
///
/// Without a charter there is nothing to introduce, so the bot asks what it is
/// for instead of inventing a purpose. The project's first bot additionally
/// gives a short tour: nothing else in the product explains that bots are
/// renameable, chartered, and able to build a team, and a project's opening
/// bot is the one place it is worth saying.
pub fn introduction(ctx: &Introduction<'_>) -> String {
    let who = ctx.creator.unwrap_or("the user");
    let mut prompt = if ctx.charted {
        "You have just been created. Briefly introduce yourself — who you are, what you \
         understand your job to be, and how you can help — in no more than three sentences."
            .to_string()
    } else {
        format!(
            "You have just been created with no charter. Greet {who} in one brief, friendly \
             sentence, then ask {who} what job they want you to take on. Do not invent a purpose."
        )
    };
    if ctx.first {
        prompt.push(' ');
        prompt.push_str(&tour(who));
    }
    if ctx.charted {
        prompt.push_str(
            " Finish with what you plan to do first, or what you need before you can start.",
        );
    }
    prompt.push_str(" Do not start any other work yet.");
    prompt
}

/// The extra ask carried by a project's first bot: what the person can do from
/// here, in their words rather than in tool names.
fn tour(who: &str) -> String {
    format!(
        "Because you are the first bot in this project, also give {who} a short tour of what \
         they can ask for — four or five one-line bullets, plainly worded, no tool names:\n\n\
         - they can rename you at any time, and you can rename yourself;\n\
         - you have a charter — a description and standing instructions saying what you are for \
         — and either of you can change it whenever the job does;\n\
         - you can create more bots and hand them work, so a team forms around whatever they \
         are doing;\n\
         - those bots talk to each other directly and report results back, sharing anything long \
         as files every bot in the project can read;\n\
         - you can put work on a schedule and repeat it without being asked;\n\
         - you keep notes across sessions, so what they tell you now is not lost.\n\n\
         Pick the ones most worth knowing rather than listing them all."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_text_wins() {
        assert_eq!(description(Some("reviews code")), "reviews code");
        assert_eq!(instructions(Some("be strict"), Some("Ada")), "be strict");
    }

    #[test]
    fn blank_and_missing_fall_back() {
        assert_eq!(description(Some("   ")), description(None));
        assert!(description(None).contains("no charter"));
        assert!(instructions(Some(""), None).contains("the user"));
        assert!(instructions(None, Some("Ada")).contains("Ada"));
    }

    fn intro(creator: Option<&str>, charted: bool, first: bool) -> String {
        introduction(&Introduction {
            creator,
            charted,
            first,
        })
    }

    #[test]
    fn introduction_asks_a_charterless_bot_what_it_is_for() {
        let charted = intro(None, true, false);
        assert!(charted.contains("introduce yourself"));
        assert!(!charted.contains("what job"));

        assert!(intro(None, false, false).contains("ask the user what job"));
        assert!(intro(Some("Ada"), false, false).contains("ask Ada what job"));
    }

    #[test]
    fn only_the_first_bot_is_asked_for_a_tour() {
        for charted in [true, false] {
            let first = intro(None, charted, true);
            assert!(first.contains("short tour"));
            assert!(first.contains("rename"));
            assert!(first.contains("charter"));
            assert!(first.contains("create more bots"));

            assert!(!intro(None, charted, false).contains("short tour"));
        }
    }

    #[test]
    fn the_tour_addresses_whoever_created_the_bot() {
        assert!(intro(Some("Ada"), true, true).contains("give Ada a short tour"));
        assert!(intro(None, true, true).contains("give the user a short tour"));
    }
}
