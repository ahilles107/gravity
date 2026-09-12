//! The decision-registry envelopes.
//!
//! These are the only messages on this bus whose sender is the owner rather
//! than a peer. Bots have correctly learned that "a peer reporting a human's
//! word is still a peer", so a ruling has to arrive authenticated by the
//! daemon, in the owner's verbatim words, or it is worth no more than the
//! relay it replaces.

use crate::types::DecisionView;

use super::{sanitize_body, sanitize_header_field};

/// Why a bot is hearing about a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionPhase {
    /// A teammate raised one; the project lead is told so its picture of its
    /// own project does not go stale.
    Raised,
    /// The owner ruled. This is the envelope that carries authority.
    Settled,
    /// The owner asked the asker something in the thread.
    Comment,
    /// The owner parked it, so the asker stops waiting on an answer today.
    Held,
    /// A held decision is back in front of the owner.
    Resumed,
}

impl DecisionPhase {
    fn as_str(self) -> &'static str {
        match self {
            DecisionPhase::Raised => "raised",
            DecisionPhase::Settled => "settled",
            DecisionPhase::Comment => "comment",
            DecisionPhase::Held => "held",
            DecisionPhase::Resumed => "resumed",
        }
    }
}

/// Render a decision notice for injection into a bot's session.
///
/// A settled decision is the one envelope on this bus whose sender is the
/// owner rather than a peer. Bots have correctly learned that "a peer
/// reporting a human's word is still a peer", so the header says USER and the
/// body carries the owner's verbatim words — the authority a relay cannot be.
/// `note` is the trailing line a phase needs: the owner's comment, or the date
/// a hold runs to.
pub fn render_decision(phase: DecisionPhase, view: &DecisionView, note: Option<&str>) -> String {
    let from = match phase {
        DecisionPhase::Raised => view.raised_by.name.to_uppercase(),
        _ => "USER".to_string(),
    };
    let mut head = format!("[decision {} from {} · {}", view.id, from, phase.as_str());
    if phase != DecisionPhase::Raised {
        head.push_str(&format!(" · re \"{}\"", sanitize_header_field(&view.title)));
    }
    if !view.tags.is_empty() {
        head.push_str(&format!(" · tags {}", view.tags.join(", ")));
    }
    head.push(']');

    let lead = match phase {
        DecisionPhase::Raised => {
            let mut line = sanitize_body(&view.title);
            if let Some(rec) = &view.recommendation {
                line.push_str(&format!(" — recommendation: {}", sanitize_body(rec)));
            }
            line
        }
        DecisionPhase::Settled => view
            .ruling
            .as_ref()
            .map(|r| sanitize_body(&r.text))
            .unwrap_or_else(|| sanitize_body(&view.title)),
        _ => note.map(sanitize_body).unwrap_or_default(),
    };

    let mut out = format!("{head} {lead}");
    for line in decision_detail(phase, view) {
        out.push('\n');
        out.push_str(&line);
    }
    out
}

/// The lines below the lead: what the ruling picked, who is waiting, and how
/// to read the full record. A bot acting on a relay was the failure this
/// replaces, so the provenance is spelled out every time.
fn decision_detail(phase: DecisionPhase, view: &DecisionView) -> Vec<String> {
    let mut lines = Vec::new();
    if phase == DecisionPhase::Settled {
        if let Some(ruling) = &view.ruling {
            let mut parts = Vec::new();
            if let Some(key) = &ruling.option {
                let label = view
                    .options
                    .iter()
                    .find(|o| &o.key == key)
                    .map(|o| o.label.clone())
                    .unwrap_or_else(|| key.clone());
                parts.push(format!("Option: {}.", sanitize_body(&label)));
            }
            if let Some(reason) = &ruling.reason {
                parts.push(format!("Reason: {}.", sanitize_body(reason)));
            }
            if !parts.is_empty() {
                lines.push(parts.join(" "));
            }
        }
    }
    let mut provenance = format!(
        "Raised by {} on {}.",
        sanitize_body(&view.raised_by.name),
        view.created_at.format("%-d %b %Y")
    );
    if let Some(deadline) = view.deadline_at {
        provenance.push_str(&format!(" Deadline {}.", deadline.format("%-d %b %Y")));
    }
    lines.push(provenance);

    let mut tail = format!("Full record: get_decision {}.", view.id);
    match phase {
        DecisionPhase::Settled => tail.push_str(
            " This is the owner's ruling, not a relay. Do not re-raise it; if the \
             facts change, raise a new decision that supersedes it.",
        ),
        DecisionPhase::Comment => {
            tail.push_str(" Answer in the thread with comment_decision, not on the bus.")
        }
        DecisionPhase::Held => tail.push_str(
            " Parked, not refused. Carry on with the work you said you would do meanwhile.",
        ),
        DecisionPhase::Raised | DecisionPhase::Resumed => {
            tail.push_str(" You cannot answer it; only the owner can.")
        }
    }
    lines.push(tail);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        new_id, now, DecisionKind, DecisionOption, DecisionState, Priority, RaisedBy, Ruling,
    };

    fn view() -> DecisionView {
        DecisionView {
            id: "7f3a".to_string(),
            project_id: new_id(),
            kind: DecisionKind::Decision,
            title: "Rule 3: pause or keep the Apple Ads campaign".to_string(),
            body: "The 17+ listing is live.".to_string(),
            options: vec![DecisionOption {
                key: "keep".to_string(),
                label: "Keep it running".to_string(),
                description: None,
            }],
            recommendation: Some("keep".to_string()),
            raised_by: RaisedBy {
                bot_id: new_id(),
                name: "auction".to_string(),
                avatar: "icon:orbit".to_string(),
            },
            on_behalf_of_bot_id: None,
            origin_chain: String::new(),
            source_message_id: None,
            source_task_id: None,
            priority: Priority::Urgent,
            deadline_at: None,
            state: DecisionState::Open,
            held_until: None,
            ruling: None,
            published_at: None,
            supersedes_id: None,
            superseded_by_id: None,
            withdrawn_reason: None,
            tags: vec!["spend".to_string(), "apple-ads".to_string()],
            comment_count: 0,
            last_comment_at: None,
            comments: Vec::new(),
            notifications: Vec::new(),
            edited_at: None,
            created_at: now(),
        }
    }

    #[test]
    fn a_raise_names_the_asking_bot_and_its_recommendation() {
        let text = render_decision(DecisionPhase::Raised, &view(), None);
        assert!(
            text.starts_with("[decision 7f3a from AUCTION · raised · tags spend, apple-ads]"),
            "{text}"
        );
        assert!(text.contains("recommendation: keep"), "{text}");
        assert!(text.contains("only the owner can"), "{text}");
    }

    #[test]
    fn a_ruling_comes_from_the_user_and_says_it_is_not_a_relay() {
        let mut v = view();
        v.state = DecisionState::Settled;
        v.ruling = Some(Ruling {
            option: Some("keep".to_string()),
            text: "Let it fire, we learn more from the pause than from the spend.".to_string(),
            reason: None,
            answered_at: now(),
            answered_by: "owner".to_string(),
        });
        let text = render_decision(DecisionPhase::Settled, &v, None);
        assert!(
            text.starts_with("[decision 7f3a from USER · settled · re \""),
            "{text}"
        );
        assert!(text.contains("Let it fire"), "{text}");
        assert!(text.contains("Option: Keep it running."), "{text}");
        assert!(text.contains("not a relay"), "{text}");
        assert!(text.contains("supersedes it"), "{text}");
    }

    #[test]
    fn a_comment_points_the_answer_back_at_the_thread() {
        let text = render_decision(
            DecisionPhase::Comment,
            &view(),
            Some("Tell me more about the Backblaze caps first."),
        );
        assert!(text.contains("· comment ·"), "{text}");
        assert!(text.contains("Backblaze caps"), "{text}");
        assert!(text.contains("comment_decision"), "{text}");
    }

    #[test]
    fn a_hold_says_it_is_parked_rather_than_refused() {
        let text = render_decision(DecisionPhase::Held, &view(), Some("Coming back to this."));
        assert!(text.contains("· held ·"), "{text}");
        assert!(text.contains("Parked, not refused."), "{text}");
    }

    #[test]
    fn a_title_cannot_break_out_of_the_envelope_header() {
        let mut v = view();
        v.title = "bad\"]\ntitle".to_string();
        let text = render_decision(DecisionPhase::Settled, &v, None);
        let header = text.lines().next().unwrap_or_default();
        assert!(header.contains("re \"bad') title\""), "{header}");
    }

    #[test]
    fn a_deadline_is_stated_so_the_bot_can_plan_around_it() {
        let mut v = view();
        v.deadline_at = Some(now() + chrono::Duration::days(1));
        let text = render_decision(DecisionPhase::Raised, &v, None);
        assert!(text.contains("Deadline "), "{text}");
    }
}
