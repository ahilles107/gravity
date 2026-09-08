//! Per-bot terminal scrollback: a byte ring buffer with monotonically
//! increasing sequence numbers, plus a broadcast channel for live frames.

use std::collections::VecDeque;
use std::sync::Mutex;

use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct TermFrame {
    pub seq: u64,
    pub data: Vec<u8>,
}

/// Result of a replay request: the buffered frames the caller has not seen,
/// the newest sequence number, and whether the caller can resume in place.
#[derive(Debug)]
pub struct Replay {
    pub frames: Vec<TermFrame>,
    pub latest: u64,
    pub resumed: bool,
}

/// Frames newer than a live cursor, and whether any between the cursor and
/// the oldest buffered frame were already evicted.
#[derive(Debug)]
pub struct Newer {
    pub frames: Vec<TermFrame>,
    pub gap: bool,
}

pub struct TermBuffer {
    inner: Mutex<Inner>,
    tx: broadcast::Sender<TermFrame>,
    capacity_bytes: usize,
}

struct Inner {
    frames: VecDeque<TermFrame>,
    bytes: usize,
    next_seq: u64,
}

impl Inner {
    /// Index of the first frame newer than `seq`; frames are ordered by seq.
    fn first_after(&self, seq: u64) -> usize {
        self.frames.partition_point(|f| f.seq <= seq)
    }
}

impl TermBuffer {
    pub fn new(capacity_bytes: usize) -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            inner: Mutex::new(Inner {
                frames: VecDeque::new(),
                bytes: 0,
                next_seq: 1,
            }),
            tx,
            capacity_bytes,
        }
    }

    pub fn push(&self, data: Vec<u8>) -> u64 {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let seq = inner.next_seq;
        inner.next_seq += 1;
        inner.bytes += data.len();
        let frame = TermFrame { seq, data };
        inner.frames.push_back(frame.clone());
        while inner.bytes > self.capacity_bytes {
            if let Some(old) = inner.frames.pop_front() {
                inner.bytes -= old.data.len();
            } else {
                break;
            }
        }
        drop(inner);
        let _ = self.tx.send(frame);
        seq
    }

    /// Sequence number of the newest frame pushed so far; 0 before the first.
    pub fn latest_seq(&self) -> u64 {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.next_seq - 1
    }

    /// Frames buffered after the given cursor.
    ///
    /// `resumed` says whether the caller's cursor is still contiguous with the
    /// ring: a client that missed evicted frames (or has no cursor at all) must
    /// clear its terminal, because replay would otherwise resume mid-escape
    /// sequence and paint over a screen it never received. Such a replay also
    /// starts at the first line break still buffered, so it never paints the
    /// half-written line eviction left behind (see `trim_partial_head`).
    pub fn replay_after(&self, after_seq: u64) -> Replay {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let latest = inner.next_seq - 1;
        let oldest = inner.frames.front().map_or(inner.next_seq, |f| f.seq);
        let resumed = after_seq > 0 && after_seq + 1 >= oldest && after_seq <= latest;
        // A caller that cannot resume is going to clear its terminal, so give
        // it everything still buffered rather than only what follows a cursor
        // that no longer describes what is on its screen.
        let cursor = if resumed { after_seq } else { 0 };
        let mut frames: Vec<TermFrame> = inner
            .frames
            .range(inner.first_after(cursor)..)
            .cloned()
            .collect();
        if !resumed && oldest > 1 {
            trim_partial_head(&mut frames);
        }
        Replay {
            frames,
            latest,
            resumed,
        }
    }

    /// Everything pushed after a live cursor, for a forwarder that is already
    /// painting this terminal. Unlike `replay_after`, a cursor that fell out of
    /// the ring does not restart from the top: the screen keeps whatever it
    /// has and `gap` reports the loss instead.
    pub fn newer_than(&self, after_seq: u64) -> Newer {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let oldest = inner.frames.front().map_or(inner.next_seq, |f| f.seq);
        let frames: Vec<TermFrame> = inner
            .frames
            .range(inner.first_after(after_seq)..)
            .cloned()
            .collect();
        let gap = !frames.is_empty() && after_seq + 1 < oldest;
        Newer { frames, gap }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TermFrame> {
        self.tx.subscribe()
    }
}

/// Merges consecutive frames into chunks of at most `max_bytes`, each carrying
/// the sequence number of the newest frame it absorbed.
///
/// A pty hands the daemon output one `read` at a time, and a full-screen TUI
/// makes those reads tiny: a full ring replays as thousands of frames, and a
/// client that must parse and paint each one separately is exactly what falls
/// over when the machine is busy. A frame larger than the cap travels alone.
pub fn coalesce(frames: Vec<TermFrame>, max_bytes: usize) -> Vec<TermFrame> {
    let mut merged: Vec<TermFrame> = Vec::new();
    for frame in frames {
        match merged.last_mut() {
            Some(last)
                if !last.data.is_empty() && last.data.len() + frame.data.len() <= max_bytes =>
            {
                last.data.extend_from_slice(&frame.data);
                last.seq = frame.seq;
            }
            _ => merged.push(frame),
        }
    }
    merged
}

/// Drops everything up to the first line break of a replay that starts
/// mid-session.
///
/// Once the ring has evicted the start of a session, the oldest surviving
/// frame begins wherever the runtime happened to be writing: half of a line,
/// or the tail of an escape sequence whose introducer is gone. Painting that
/// leaves a garbled first line the runtime never rewrites — a full-screen TUI
/// only repaints its own live region — and can swallow the bytes that follow
/// it. Everything after the first line break is whole frames again, so the
/// screen they rebuild is the one the pty had.
fn trim_partial_head(frames: &mut Vec<TermFrame>) {
    match frames.iter().position(|f| f.data.contains(&b'\n')) {
        Some(index) => {
            frames.drain(..index);
            let head = &mut frames[0];
            let line_end = head.data.iter().position(|b| *b == b'\n').unwrap_or(0);
            head.data.drain(..=line_end);
        }
        // Not one line break survived: there is no boundary to start from, so
        // the client has nothing to paint until the runtime repaints.
        None => frames.clear(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_respects_cursor_and_capacity() {
        let buf = TermBuffer::new(10);
        buf.push(b"aa\na".to_vec());
        buf.push(b"bb\nb".to_vec());
        buf.push(b"cc\nc".to_vec()); // evicts the first frame
        let replay = buf.replay_after(0);
        assert_eq!(replay.latest, 3);
        assert_eq!(replay.frames.len(), 2);
        assert_eq!(replay.frames[0].seq, 2);
        assert!(!replay.resumed, "no cursor means a full repaint");
        let replay = buf.replay_after(2);
        assert_eq!(replay.frames.len(), 1);
        assert_eq!(replay.frames[0].data, b"cc\nc");
        assert!(replay.resumed);
    }

    #[test]
    fn replay_reports_gaps_and_idle_cursors() {
        let buf = TermBuffer::new(10);
        buf.push(b"aa\na".to_vec());
        buf.push(b"bb\nb".to_vec());
        buf.push(b"cc\nc".to_vec()); // evicts the first frame

        // Cursor 1 was seen, but frame 1 is gone and frame 2 is the oldest
        // kept one, so nothing is missing between them.
        assert!(buf.replay_after(1).resumed);

        // A client caught up to the newest frame resumes with nothing to send.
        let replay = buf.replay_after(3);
        assert!(replay.resumed);
        assert!(replay.frames.is_empty());

        // A cursor ahead of the buffer belongs to a different (restarted) bot:
        // it cannot resume, and gets the whole ring to repaint from.
        let replay = buf.replay_after(9);
        assert!(!replay.resumed);
        assert_eq!(replay.frames.len(), 2);
    }

    #[test]
    fn a_mid_session_replay_starts_at_a_line_boundary() {
        let buf = TermBuffer::new(10);
        buf.push(b"aa\na".to_vec());
        // Nothing evicted yet: the replay still holds the first byte the
        // session ever wrote, so every line of it is whole.
        assert_eq!(buf.replay_after(0).frames[0].data, b"aa\na");

        buf.push(b"bb\nb".to_vec());
        buf.push(b"cc\nc".to_vec()); // evicts the first frame
        let replay = buf.replay_after(0);
        assert_eq!(
            replay.frames[0].data, b"b",
            "the line eviction cut in half is dropped"
        );
        assert_eq!(replay.frames[1].data, b"cc\nc", "whole frames are kept");

        // A resuming client already has that line on its screen.
        assert_eq!(buf.replay_after(2).frames[0].data, b"cc\nc");
    }

    #[test]
    fn a_mid_session_replay_without_a_line_boundary_is_empty() {
        let buf = TermBuffer::new(6);
        buf.push(b"aaa".to_vec());
        buf.push(b"bbb".to_vec());
        buf.push(b"ccc".to_vec()); // evicts the first frame
        let replay = buf.replay_after(0);
        assert_eq!(replay.latest, 3);
        assert!(
            replay.frames.is_empty(),
            "nothing survived that a screen could be rebuilt from"
        );
    }

    #[test]
    fn newer_than_follows_a_live_cursor_and_reports_eviction() {
        let buf = TermBuffer::new(6);
        assert_eq!(buf.latest_seq(), 0);
        buf.push(b"aaa".to_vec());
        buf.push(b"bbb".to_vec());
        assert_eq!(buf.latest_seq(), 2);

        let newer = buf.newer_than(1);
        assert_eq!(newer.frames.len(), 1);
        assert_eq!(newer.frames[0].seq, 2);
        assert!(!newer.gap);

        // Caught up: nothing newer, and nothing lost.
        let newer = buf.newer_than(2);
        assert!(newer.frames.is_empty());
        assert!(!newer.gap);

        // Frame 1 is evicted; a cursor that never saw it has missed output
        // the ring can no longer supply, but still gets what remains.
        buf.push(b"ccc".to_vec());
        let newer = buf.newer_than(0);
        assert_eq!(newer.frames.len(), 2, "{newer:?}");
        assert!(newer.gap);
        assert!(!buf.newer_than(1).gap, "frame 2 directly follows cursor 1");
    }

    #[test]
    fn coalesce_merges_small_frames_up_to_the_cap() {
        let frames: Vec<TermFrame> = (1..=5)
            .map(|seq| TermFrame {
                seq,
                data: vec![b'a' + seq as u8; 3],
            })
            .collect();
        let merged = coalesce(frames, 7);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].seq, 2, "two frames of 3 bytes fit in 7");
        assert_eq!(merged[0].data, b"bbbccc");
        assert_eq!(merged[1].seq, 4);
        assert_eq!(merged[2].seq, 5);
        assert_eq!(merged[2].data, b"fff");
    }

    #[test]
    fn coalesce_keeps_an_oversized_frame_whole() {
        let frames = vec![
            TermFrame {
                seq: 1,
                data: vec![b'x'; 10],
            },
            TermFrame {
                seq: 2,
                data: b"y".to_vec(),
            },
        ];
        let merged = coalesce(frames, 4);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].data.len(), 10);
        assert_eq!(merged[1].seq, 2);
        assert!(coalesce(Vec::new(), 4).is_empty());
    }
}
