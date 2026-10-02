//! Picking work back up after a restart.
//!
//! Bots are always-on and come back with `--continue`, so their conversation
//! survives a daemon restart, but a turn that was running when the session
//! stopped does not resume: the bot sits idle and its open tasks wait forever.
//! At startup, before sessions are started again, this finds each bot that
//! was cut off mid-turn or still holds open tasks, and queues it one note
//! saying what it was doing and which tasks are open.

use std::sync::Arc;

use bus::{Bot, DeliveryState, MessageKind};

use crate::app::AppState;
use crate::chat::model::Trigger;
use crate::messaging::{self, daemon_sender, Dm};

/// The note's opening, which also marks it: a bot with one still waiting to
/// be delivered is not sent another.
pub const HEADER: &str = "Gravity restarted, and your session was restarted with it.";

/// Characters of a task's request quoted in the note.
const PREVIEW_CHARS: usize = 300;

/// What one bot was doing when the daemon stopped.
#[derive(Debug, Clone)]
pub struct Interrupted {
    pub bot_id: String,
    /// What started the turn that never finished, if one didn't.
    pub turn: Option<Trigger>,
    /// Open tasks assigned to the bot: `(task id, who asked, the request)`.
    pub tasks: Vec<(String, String, String)>,
}

/// The work to pick back up, read from transcripts and tasks. Run before the
/// supervisor starts sessions, while the transcripts still end where the old
/// sessions stopped.
pub fn interrupted_work(app: &AppState) -> Vec<Interrupted> {
    if !app.cfg.resume_after_restart {
        return Vec::new();
    }
    let bots = app.db.list_bots(None).unwrap_or_default();
    bots.iter()
        .filter(|bot| !bot.is_linked())
        .filter_map(|bot| match interrupted(app, bot) {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!(bot_id = %bot.id, error = %e, "could not tell what the bot was doing");
                None
            }
        })
        .collect()
}

fn interrupted(app: &AppState, bot: &Bot) -> anyhow::Result<Option<Interrupted>> {
    let turn = app.chat.interrupted(app, bot)?;
    let mut tasks = Vec::new();
    for task in app.db.open_tasks_for(&bot.id)? {
        let asked_by = match &task.from_bot_id {
            Some(id) => app.db.get_bot(id)?.map_or_else(
                || "another bot".to_string(),
                |b| crate::db::Db::display_name(&b),
            ),
            None => "the owner".to_string(),
        };
        let request = app
            .db
            .get_message(&task.origin_message_id)?
            .map(|m| crate::chat::truncate(&m.body, PREVIEW_CHARS))
            .unwrap_or_default();
        tasks.push((task.id, asked_by, request));
    }
    if turn.is_none() && tasks.is_empty() {
        return Ok(None);
    }
    Ok(Some(Interrupted {
        bot_id: bot.id.clone(),
        turn,
        tasks,
    }))
}

/// What started a turn, in a line.
fn what_started(trigger: &Trigger) -> String {
    let quote = |text: &str| crate::chat::truncate(text.trim(), PREVIEW_CHARS);
    match trigger {
        Trigger::Owner { text, .. } => format!("the owner's message: {}", quote(text)),
        Trigger::Bus {
            from,
            msg_kind,
            text,
            ..
        } => format!("{from}'s {msg_kind}: {}", quote(text)),
        Trigger::Routine { name, text, .. } => format!("your routine {name}: {}", quote(text)),
        Trigger::Ruling { text, .. } => format!("a decision ruling: {}", quote(text)),
        Trigger::Resumed => "work you had resumed after a compaction".to_string(),
        Trigger::Background { text } => format!("a background event: {}", quote(text)),
    }
}

/// The note a bot gets.
pub fn note(work: &Interrupted) -> String {
    let mut body = HEADER.to_string();
    if let Some(turn) = &work.turn {
        body.push_str(&format!(
            "\n\nYou were in the middle of a turn, started by {}",
            what_started(turn)
        ));
    }
    if !work.tasks.is_empty() {
        body.push_str("\n\nOpen tasks assigned to you:");
        for (id, asked_by, request) in &work.tasks {
            body.push_str(&format!("\n- task_id {id}, from {asked_by}: {request}"));
        }
    }
    body.push_str(
        "\n\nPick up where you left off. Anything that was running when the \
         session stopped (a command, a build, a background job) has stopped, \
         so check the state of your files first. Finish each task with \
         complete_task. If you were waiting on someone, keep waiting: nothing \
         else is needed.",
    );
    body
}

/// Queues each bot its note. The delivery worker holds it until the bot's new
/// session is ready, as for any message.
pub fn nudge(app: &Arc<AppState>, work: Vec<Interrupted>) {
    for item in work {
        if already_told(app, &item.bot_id) {
            continue;
        }
        let body = note(&item);
        match messaging::send_dm(
            &app.db,
            &app.events,
            Dm::new(&item.bot_id, &daemon_sender(), MessageKind::Note, &body),
        ) {
            Ok(_) => tracing::info!(bot_id = %item.bot_id, tasks = item.tasks.len(),
                interrupted = item.turn.is_some(), "bot told to pick its work back up"),
            Err(e) => tracing::warn!(bot_id = %item.bot_id, error = %e, "resume note not queued"),
        }
    }
}

/// Whether a resume note is still waiting to reach the bot, as after two
/// restarts in quick succession.
fn already_told(app: &AppState, bot_id: &str) -> bool {
    let pending = [DeliveryState::Queued, DeliveryState::Leased]
        .into_iter()
        .flat_map(|state| {
            app.db
                .list_deliveries(Some(bot_id), Some(state))
                .unwrap_or_default()
        });
    pending
        .filter_map(|delivery| app.db.get_message(&delivery.message_id).ok().flatten())
        .any(|message| message.body.starts_with(HEADER))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_note_says_what_was_running_and_what_is_open() {
        let work = Interrupted {
            bot_id: "b".to_string(),
            turn: Some(Trigger::Bus {
                from: "lead".to_string(),
                msg_kind: "task".to_string(),
                num: 3,
                text: "port the updater".to_string(),
                task_id: None,
            }),
            tasks: vec![(
                "t1".to_string(),
                "lead".to_string(),
                "port the updater".to_string(),
            )],
        };
        let text = note(&work);
        assert!(text.starts_with(HEADER));
        assert!(text.contains("started by lead's task: port the updater"));
        assert!(text.contains("- task_id t1, from lead: port the updater"));
        assert!(text.contains("keep waiting"));
        let idle = note(&Interrupted { turn: None, ..work });
        assert!(!idle.contains("middle of a turn"));
    }
}
