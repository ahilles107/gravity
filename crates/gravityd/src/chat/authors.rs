//! Who made each file in a project's artifacts, for the Files tab.
//!
//! - A file a bot wrote or edited with its file tools is the first such
//!   bot's, by its transcript.
//! - Otherwise, a file named in a command is the bot whose command first
//!   mentioned its path (a build, a copy, an image render).
//! - `uploads/` holds what the owner attached from the app.
//! - `peers/<machine>/<message>/` holds files a bot on another machine sent
//!   with a result, which the message traces to that bot.

use std::collections::HashMap;

use bus::{Bot, Project};
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::app::AppState;
use crate::db::Db;

use super::files::Artifact;
use super::writes::Touch;

#[derive(Debug, Clone, Serialize)]
pub struct Creator {
    /// The bot, or `None` for the owner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<String>,
    pub name: String,
    /// The bot's avatar; empty for the owner.
    pub avatar: String,
    /// The peer the bot runs on, for one on another machine.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    /// How: `wrote`, `edited`, `command`, `upload` or `sent`.
    pub via: &'static str,
}

fn bot_creator(app: &AppState, bot: &Bot, via: &'static str) -> Creator {
    let machine = bot
        .peer_id
        .as_deref()
        .and_then(|peer| app.db.get_peer(peer).ok().flatten())
        .map(|peer| Db::display_peer_name(&peer));
    Creator {
        bot_id: Some(bot.id.clone()),
        name: Db::display_name(bot),
        avatar: bot.avatar.clone(),
        machine,
        via,
    }
}

/// Fills in `created_by` wherever it can be told.
pub fn attribute(app: &AppState, project: &Project, artifacts: &mut [Artifact]) {
    let mut earliest: HashMap<usize, (DateTime<Utc>, Creator)> = HashMap::new();
    let bots = app.db.list_bots(Some(&project.id)).unwrap_or_default();
    for bot in bots.iter().filter(|b| !b.is_linked()) {
        let found = app.chat.with_logs(app, bot, |commands, writes| {
            artifacts
                .iter()
                .enumerate()
                .filter_map(|(i, artifact)| {
                    let touched = writes.first(&artifact.path).map(|(at, touch)| {
                        let via = if touch == Touch::Wrote {
                            "wrote"
                        } else {
                            "edited"
                        };
                        (at, via)
                    });
                    let found = touched.or_else(|| {
                        commands
                            .first_mention(&artifact.path)
                            .map(|at| (at, "command"))
                    });
                    found.map(|(at, via)| (i, at, via))
                })
                .collect::<Vec<_>>()
        });
        let Ok(found) = found else {
            continue;
        };
        // A file tool beats a command that only mentioned the path; then the
        // earliest wins.
        let rank = |via: &str, at: DateTime<Utc>| (via == "command", at);
        for (i, at, via) in found {
            let better = earliest
                .get(&i)
                .is_none_or(|(seen, creator)| rank(via, at) < rank(creator.via, *seen));
            if better {
                earliest.insert(i, (at, bot_creator(app, bot, via)));
            }
        }
    }
    for (i, artifact) in artifacts.iter_mut().enumerate() {
        artifact.created_by = earliest
            .remove(&i)
            .map(|(_, creator)| creator)
            .or_else(|| by_place(app, &artifact.rel));
    }
}

/// Files whose folder says who made them: uploads, and a peer's results.
fn by_place(app: &AppState, rel: &str) -> Option<Creator> {
    let mut parts = rel.split('/');
    match parts.next()? {
        "uploads" => Some(Creator {
            bot_id: None,
            name: "you".to_string(),
            avatar: String::new(),
            machine: None,
            via: "upload",
        }),
        "peers" => {
            let (machine, message) = (parts.next()?, parts.next()?);
            let peer = app
                .db
                .list_peers()
                .ok()?
                .into_iter()
                .find(|p| bus::names::dir_name(&p.name) == machine)?;
            let local = app.db.local_message_by_prefix(&peer.id, message).ok()??;
            let sender = app.db.get_message(&local).ok()??.sender.bot_id?;
            let bot = app.db.get_bot(&sender).ok()??;
            Some(bot_creator(app, &bot, "sent"))
        }
        _ => None,
    }
}
