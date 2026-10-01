//! Reading back the envelopes the daemon delivers. A delivered message lands
//! in the transcript as user input; this turns it back into who sent what.
//! The formats are the ones `bus::envelope` renders.

/// What Claude Code wraps around a message delivered over its inbox socket.
const PEER_PREFIX: &str = "Another Claude session sent a message:\n";
/// The start of the trailer Claude Code appends after it.
const PEER_TRAILER: &str = "\n\nThis came from another Claude session";

#[derive(Debug, PartialEq, Eq)]
pub enum Delivered {
    Message {
        num: i64,
        /// The sender as the envelope names it: `USER`, or a bot name in
        /// upper case.
        from: String,
        kind: String,
        task_id: Option<String>,
        body: String,
    },
    Routine {
        name: String,
        run_id: Option<String>,
        prompt: String,
    },
    Decision {
        decision_id: String,
        text: String,
    },
}

/// The delivered text inside Claude Code's peer-message wrapper, or the text
/// itself when it is not wrapped (Codex observations, older transcripts).
pub fn unwrap_peer(text: &str) -> &str {
    let inner = text.strip_prefix(PEER_PREFIX).unwrap_or(text);
    match inner.find(PEER_TRAILER) {
        Some(end) => &inner[..end],
        None => inner,
    }
}

pub fn parse(text: &str) -> Option<Delivered> {
    let text = unwrap_peer(text).trim_start();
    if let Some(rest) = text.strip_prefix("[msg #") {
        return parse_message(rest);
    }
    if let Some(rest) = text.strip_prefix("[routine \"") {
        return parse_routine(rest);
    }
    if let Some(rest) = text.strip_prefix("[decision ") {
        let decision_id = rest.split_whitespace().next()?.to_string();
        return Some(Delivered::Decision {
            decision_id,
            text: text.to_string(),
        });
    }
    None
}

/// `N from NAME · kind[ · re #M][ · task_id ID]] body`
fn parse_message(rest: &str) -> Option<Delivered> {
    let (head, body) = rest.split_once(']')?;
    let mut parts = head.split(" · ");
    let (num, from) = parts.next()?.split_once(" from ")?;
    let kind = parts.next()?.trim().to_string();
    let mut task_id = None;
    for part in parts {
        if let Some(id) = part.strip_prefix("task_id ") {
            task_id = Some(id.trim().to_string());
        }
    }
    Some(Delivered::Message {
        num: num.trim().parse().ok()?,
        from: from.trim().to_string(),
        kind,
        task_id,
        body: body.strip_prefix(' ').unwrap_or(body).to_string(),
    })
}

/// `name" #N[ · run_id ID]] prompt`
fn parse_routine(rest: &str) -> Option<Delivered> {
    let (name, rest) = rest.split_once('"')?;
    let (head, prompt) = rest.split_once(']')?;
    let run_id = head
        .split(" · ")
        .find_map(|part| part.strip_prefix("run_id "))
        .map(|id| id.trim().to_string());
    Some(Delivered::Routine {
        name: name.to_string(),
        run_id,
        prompt: prompt.strip_prefix(' ').unwrap_or(prompt).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_wrapped_task_with_its_id() {
        let text = "Another Claude session sent a message:\n[msg #12 from LEAD · task · re #3 \
                    · task_id t-1] Port the updater.\nLine two.\n\nThis came from another \
                    Claude session — not typed by your user.";
        assert_eq!(
            parse(text),
            Some(Delivered::Message {
                num: 12,
                from: "LEAD".to_string(),
                kind: "task".to_string(),
                task_id: Some("t-1".to_string()),
                body: "Port the updater.\nLine two.".to_string(),
            })
        );
    }

    #[test]
    fn reads_the_owner_chat_and_a_routine() {
        assert!(matches!(
            parse("[msg #4 from USER · chat] hi"),
            Some(Delivered::Message { from, kind, .. }) if from == "USER" && kind == "chat"
        ));
        assert_eq!(
            parse("[routine \"nightly\" #7 · run_id r-9] /report"),
            Some(Delivered::Routine {
                name: "nightly".to_string(),
                run_id: Some("r-9".to_string()),
                prompt: "/report".to_string(),
            })
        );
    }

    #[test]
    fn plain_text_is_not_an_envelope() {
        assert_eq!(parse("just typed this"), None);
    }
}
