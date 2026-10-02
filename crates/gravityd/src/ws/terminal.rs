//! Terminal attach, detach, input and resize requests. There is no input
//! lease: the terminal belongs to the user, and typing is gated only by the
//! `control` grant. Bus deliveries go through each session's inbox socket
//! and never touch the terminal.
//!
//! A linked bot's terminal is its peer's, mirrored into the stand-in's buffer
//! while anyone watches (see `crate::peer::term`): attaching works the same,
//! and input and resizes are relayed to the real one.

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::UnboundedSender;

use crate::terminal::{coalesce, TermBuffer, TermFrame};

use super::Conn;

/// Largest `term` push, in output bytes. Small enough to parse and paint in
/// one go on the client, large enough that a full ring replay is a handful of
/// frames rather than one per pty read.
const MAX_TERM_PUSH_BYTES: usize = 64 * 1024;

fn term_push(bot_id: &str, frame: &TermFrame) -> Value {
    json!({
        "type": "term", "bot_id": bot_id, "seq": frame.seq,
        "data": String::from_utf8_lossy(&frame.data)
    })
}

/// Sends every frame after `last_seq` that the ring still holds, merged into
/// as few pushes as the cap allows, and returns the newest sequence sent.
fn forward_newer(
    out: &UnboundedSender<Value>,
    term: &TermBuffer,
    bot_id: &str,
    mut last_seq: u64,
) -> Result<u64, ()> {
    let newer = term.newer_than(last_seq);
    if newer.gap {
        // Only reachable when this forwarder fell a whole ring behind. The
        // screen keeps what it has; the runtime's next repaint rebuilds it.
        tracing::warn!(bot_id, last_seq, "terminal forwarder fell behind the ring");
    }
    for frame in coalesce(newer.frames, MAX_TERM_PUSH_BYTES) {
        last_seq = frame.seq;
        out.send(term_push(bot_id, &frame)).map_err(|_| ())?;
    }
    Ok(last_seq)
}

/// Forwards live frames until the client goes away.
///
/// The broadcast channel is only a wakeup: what actually goes out is read
/// back from the ring, so everything that arrived since the last push travels
/// merged, and a lagging receiver loses nothing the ring still holds.
async fn forward_live(
    out: UnboundedSender<Value>,
    term: Arc<TermBuffer>,
    bot_id: String,
    mut last_seq: u64,
    mut rx: Receiver<TermFrame>,
) {
    loop {
        match rx.recv().await {
            Ok(frame) if frame.seq <= last_seq => continue,
            Ok(_) => {}
            Err(RecvError::Lagged(skipped)) => {
                tracing::debug!(bot_id, skipped, "terminal wakeups lagged; reading the ring");
            }
            Err(RecvError::Closed) => break,
        }
        match forward_newer(&out, &term, &bot_id, last_seq) {
            Ok(seq) => last_seq = seq,
            Err(()) => break,
        }
    }
}

/// Answers an attach and replays what the ring holds after `after_seq`;
/// returns the cursor and receiver the live forwarder continues from.
fn replay(
    out: &UnboundedSender<Value>,
    term: &TermBuffer,
    bot_id: &str,
    after_seq: u64,
    req_id: &Value,
) -> (u64, Receiver<TermFrame>) {
    // Subscribe before the replay and pass this receiver to the forwarder,
    // so a frame landing in between is in the replay or wakes the forwarder,
    // which skips anything at or below the replay cursor.
    let rx = term.subscribe();
    let replay = term.replay_after(after_seq);
    let _ = out.send(json!({
        "type": "attached", "req_id": req_id, "bot_id": bot_id,
        "seq": replay.latest, "resumed": replay.resumed
    }));
    for frame in coalesce(replay.frames, MAX_TERM_PUSH_BYTES) {
        let _ = out.send(term_push(bot_id, &frame));
    }
    (replay.latest, rx)
}

impl Conn {
    // ---- terminal ----

    pub(super) fn attach(&mut self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?.to_string();
        let after_seq = req.get("after_seq").and_then(|v| v.as_u64()).unwrap_or(0);
        let bot = self
            .app
            .db
            .get_bot(&bot_id)?
            .ok_or_else(|| anyhow::anyhow!("bot not found"))?;

        // Drop an earlier attachment to this bot first: its forwarder writes to
        // the same connection from its own task, so leaving it running would
        // interleave live frames with the replay below and paint over a screen
        // the client is still rebuilding.
        if let Some(old) = self.attachments.remove(&bot_id) {
            old.abort();
        }
        let (app, out, req_id) = (self.app.clone(), self.out.clone(), req_id.clone());
        let task = if bot.is_linked() {
            // The peer feeds the mirror for as long as this task holds its
            // viewer; detaching aborts the task and lets it go.
            tokio::spawn(async move {
                match crate::peer::term::view(&app, &bot).await {
                    Ok(viewer) => {
                        let term = app.supervisor.ensure_term(&bot.id);
                        let (latest, rx) = replay(&out, &term, &bot.id, after_seq, &req_id);
                        forward_live(out, term, bot.id, latest, rx).await;
                        drop(viewer);
                    }
                    Err(e) => {
                        let _ = out.send(json!({
                            "type": "error", "req_id": req_id, "code": "unavailable",
                            "message": format!("{}'s terminal: {e:#}", bot.name)
                        }));
                    }
                }
            })
        } else {
            let term = app.supervisor.ensure_term(&bot_id);
            let (latest, rx) = replay(&out, &term, &bot_id, after_seq, &req_id);
            tokio::spawn(forward_live(out, term, bot_id.clone(), latest, rx))
        };
        self.attachments.insert(bot_id, task);
        Ok(())
    }

    pub(super) fn detach(&mut self, req_id: &Value, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        if let Some(task) = self.attachments.remove(bot_id) {
            task.abort();
        }
        self.send(json!({ "type": "ok", "req_id": req_id }));
        Ok(())
    }

    pub(super) fn input(&self, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let data = Self::str_field(req, "data")?;
        if let Some(bot) = self.app.db.get_bot(bot_id)?.filter(bus::Bot::is_linked) {
            crate::peer::term::input(&self.app, &bot, data);
            return Ok(());
        }
        if let Err(e) = self.app.supervisor.input(bot_id, data.as_bytes()) {
            self.reply_err(&Value::Null, "runtime_unavailable", &e.to_string());
        }
        Ok(())
    }

    pub(super) fn resize(&self, req: &Value) -> anyhow::Result<()> {
        let bot_id = Self::str_field(req, "bot_id")?;
        let cols = req.get("cols").and_then(|v| v.as_u64()).unwrap_or(120) as u16;
        let rows = req.get("rows").and_then(|v| v.as_u64()).unwrap_or(36) as u16;
        // A client sets `force` right after attaching: even when the size is
        // unchanged it needs the full-screen runtime to repaint, because
        // replay alone cannot rebuild a screen older than the ring buffer.
        let force = req.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
        if let Some(bot) = self.app.db.get_bot(bot_id)?.filter(bus::Bot::is_linked) {
            crate::peer::term::resize(&self.app, &bot, cols, rows, force);
            return Ok(());
        }
        let _ = self.app.supervisor.resize(bot_id, cols, rows, force);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn forwards_output_written_before_the_live_task_starts() {
        let term = Arc::new(TermBuffer::new(1024));
        let rx = term.subscribe();
        term.push(b"replay".to_vec());
        let replay = term.replay_after(0);
        let (out, mut received) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::spawn(forward_live(
            out,
            term.clone(),
            "bot".into(),
            replay.latest,
            rx,
        ));

        // On this current-thread runtime the task cannot start until we await.
        // No later write should be needed to wake it, and replay stays skipped.
        let seq = term.push(b"live".to_vec());
        let frame = tokio::time::timeout(std::time::Duration::from_secs(1), received.recv())
            .await
            .expect("live output must not wait for another write")
            .expect("terminal push");
        assert_eq!(
            frame,
            json!({"type": "term", "bot_id": "bot", "seq": seq, "data": "live"})
        );
        assert!(
            received.try_recv().is_err(),
            "replay must not be forwarded again"
        );
        task.abort();
    }
}
