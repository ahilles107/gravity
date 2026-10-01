use super::{pump, utf8_prefix_len, COALESCE_WINDOW, MAX_FRAME_BYTES};
use std::collections::VecDeque;
use std::io::Read;

/// A reader that returns each queued write as its own `read`, the way a
/// pty hands over one small write at a time.
struct Writes(VecDeque<Vec<u8>>);

impl Read for Writes {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let Some(mut write) = self.0.pop_front() else {
            return Ok(0);
        };
        // A write larger than the caller's buffer comes back in pieces,
        // exactly as a pty read would return it.
        let n = write.len().min(buf.len());
        let rest = write.split_off(n);
        if !rest.is_empty() {
            self.0.push_front(rest);
        }
        buf[..n].copy_from_slice(&write);
        Ok(n)
    }
}

fn frames(writes: &[&[u8]], mut more_soon: impl FnMut() -> bool) -> Vec<Vec<u8>> {
    let mut reader = Writes(writes.iter().map(|w| w.to_vec()).collect());
    let mut out = Vec::new();
    pump(
        &mut reader,
        |_| more_soon(),
        |chunk| {
            out.push(chunk);
            true
        },
    );
    out
}

#[test]
fn merges_a_burst_of_small_writes_into_one_frame() {
    let out = frames(&[b"\x1b[2K", b"prompt", b" > ", b"\x1b[1A"], || true);
    assert_eq!(out, vec![b"\x1b[2Kprompt > \x1b[1A".to_vec()]);
}

#[test]
fn emits_separate_frames_when_output_pauses() {
    let out = frames(&[b"first", b"second"], || false);
    assert_eq!(out, vec![b"first".to_vec(), b"second".to_vec()]);
}

#[test]
fn flushes_continuous_output_when_the_batch_deadline_expires() {
    let out = frames(&[b"a", b"b", b"c"], || {
        // More data keeps arriving, but waiting for it uses up the batch's
        // time budget. It must not start another full wait for each read.
        std::thread::sleep(COALESCE_WINDOW);
        true
    });
    assert!(out.len() >= 2, "continuous output must not wait for EOF");
    assert_eq!(out.concat(), b"abc");
}

#[test]
fn caps_a_merged_frame() {
    let big = vec![b'x'; MAX_FRAME_BYTES - 1];
    let out = frames(&[&big, b"yy", b"z"], || true);
    // The time limit can split a batch sooner on a busy machine.
    assert!(out.len() >= 2, "the byte cap must split this output");
    assert!(out
        .iter()
        .all(|frame| frame.len() <= MAX_FRAME_BYTES + 8191));
    assert_eq!(out.concat(), [big, b"yyz".to_vec()].concat());
}

#[test]
fn holds_an_incomplete_character_for_the_next_read() {
    let glyph = "╭".as_bytes();
    let out = frames(&[b"a", &glyph[..1], &glyph[1..]], || false);
    assert_eq!(out, vec![b"a".to_vec(), glyph.to_vec()]);
}

#[test]
fn stops_once_the_consumer_is_gone() {
    let mut reader = Writes([b"a".to_vec(), b"b".to_vec()].into());
    let mut seen = 0;
    pump(
        &mut reader,
        |_| false,
        |_| {
            seen += 1;
            false
        },
    );
    assert_eq!(seen, 1);
}

/// The real thing: a shell writing one tiny escape sequence per syscall,
/// the way a TUI repaints, must reach the daemon as a few frames, not one
/// frame per write. Timing-dependent by nature, so the bound is loose.
#[cfg(unix)]
#[tokio::test]
async fn a_real_pty_burst_is_merged_into_few_frames() {
    use super::{PtyAdapter, RuntimeAdapter};
    use crate::runtime::{BotSpec, SessionEvent};

    const WRITES: usize = 400;
    let script = format!(
        "i=0; while [ $i -lt {WRITES} ]; do printf '\\033[2K\\033[1A%04d' $i; i=$((i+1)); done"
    );
    let spec = BotSpec {
        bot_id: "pty-test".into(),
        bot_name: "pty".into(),
        workspace: std::env::temp_dir(),
        claude_bin: "/bin/sh".into(),
        claude_args: vec!["-c".into(), script],
        env: Vec::new(),
        cols: 80,
        rows: 24,
    };
    let mut started = PtyAdapter.start(&spec).expect("spawn sh in a pty");
    let mut frames = 0usize;
    let mut bytes = 0usize;
    while let Some(event) = started.events.recv().await {
        match event {
            SessionEvent::Output(data) => {
                frames += 1;
                bytes += data.len();
            }
            SessionEvent::Exited { .. } => break,
        }
    }
    // Each write is two 4-byte escape sequences plus 4 digits.
    assert_eq!(bytes, WRITES * 12, "every byte the shell wrote arrived");
    assert!(
        frames * 4 < WRITES,
        "{WRITES} writes arrived as {frames} frames; expected far fewer"
    );
    eprintln!("real pty: {WRITES} writes -> {frames} frames ({bytes} bytes)");
}

#[test]
fn holds_back_incomplete_trailing_sequences() {
    let full = "ab╭─".as_bytes(); // '╭' and '─' are 3 bytes each
    assert_eq!(utf8_prefix_len(full), full.len());
    assert_eq!(utf8_prefix_len(&full[..full.len() - 1]), full.len() - 3);
    assert_eq!(utf8_prefix_len(&full[..full.len() - 2]), full.len() - 3);
    assert_eq!(utf8_prefix_len(&full[..full.len() - 3]), full.len() - 3);
    assert_eq!(utf8_prefix_len(b""), 0);
    assert_eq!(utf8_prefix_len(b"plain ascii"), 11);
}

#[test]
fn keeps_complete_four_byte_sequences() {
    let full = "x\u{1F600}".as_bytes();
    assert_eq!(utf8_prefix_len(full), full.len());
    assert_eq!(utf8_prefix_len(&full[..full.len() - 1]), 1);
}
