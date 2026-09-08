//! PTY runtime adapter: spawns `claude` inside a portable-pty and streams
//! terminal output. The PTY is strictly for the user's terminal — deliveries
//! go through the session's inbox socket, reported by the SessionStart hook.

use std::io::{Read, Write};
use std::os::fd::RawFd;
use std::process::Command as StdCommand;
use std::time::{Duration, Instant};

use anyhow::Context;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use tokio::sync::mpsc;

use super::{BotSpec, Capabilities, RuntimeAdapter, RuntimeSession, SessionEvent, StartedSession};

/// Upper bound on one output frame after merging consecutive reads.
const MAX_FRAME_BYTES: usize = 64 * 1024;
/// How long a frame waits for more output before it is emitted. A TUI writes
/// one screen as a burst of small writes; merging what lands within this
/// window turns thousands of frames into a handful. Keystroke echo pays at
/// most this much extra latency.
const COALESCE_WINDOW: Duration = Duration::from_millis(4);

pub struct PtyAdapter;

impl RuntimeAdapter for PtyAdapter {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            kind: "pty",
            native_background: false,
            channel_delivery: true,
            permission_relay: false,
        }
    }

    fn start(&self, spec: &BotSpec) -> anyhow::Result<StartedSession> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: spec.rows,
                cols: spec.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("openpty")?;

        let mut cmd = CommandBuilder::new(&spec.claude_bin);
        for arg in &spec.claude_args {
            cmd.arg(arg);
        }
        cmd.cwd(&spec.workspace);
        // Bots must start as fresh top-level sessions. If the daemon itself
        // was launched from inside a Claude Code session, inherited markers
        // (CLAUDECODE, CLAUDE_CODE_*) would make the bot think it is a nested
        // child session and, e.g., disable transcript saving.
        for (key, _) in std::env::vars() {
            if key == "CLAUDECODE" || key.starts_with("CLAUDE_CODE_") {
                cmd.env_remove(&key);
            }
        }
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        cmd.env("TERM", "xterm-256color");

        let child = pair.slave.spawn_command(cmd).context("spawn claude")?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().context("clone pty reader")?;
        let writer = pair.master.take_writer().context("take pty writer")?;
        let master_fd = pair.master.as_raw_fd();

        let (tx, rx) = mpsc::unbounded_channel();
        let killer = child.clone_killer();

        // Blocking reader thread; ends when the child exits and the pty EOFs.
        let out_tx = tx.clone();
        std::thread::spawn(move || {
            let mut child = child;
            let more_soon = |timeout| master_fd.is_some_and(|fd| readable_within(fd, timeout));
            pump(&mut reader, more_soon, |chunk| {
                out_tx.send(SessionEvent::Output(chunk)).is_ok()
            });
            let code = child.wait().ok().map(|s| s.exit_code() as i32);
            let _ = out_tx.send(SessionEvent::Exited { code });
        });

        Ok(StartedSession {
            session: Box::new(PtySession {
                master: pair.master,
                writer,
                killer,
            }),
            events: rx,
            // The SessionStart hook reports the inbox socket once the session
            // is up; deliveries wait until then.
            msg_socket: None,
        })
    }

    fn probe(&self) -> anyhow::Result<String> {
        let out = StdCommand::new("claude").arg("--version").output();
        match out {
            Ok(o) if o.status.success() => {
                Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
            }
            Ok(o) => anyhow::bail!(
                "claude --version failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ),
            Err(e) => anyhow::bail!("claude binary not found: {e}"),
        }
    }
}

/// Whether `fd` has output to read within `timeout`.
fn readable_within(fd: RawFd, timeout: Duration) -> bool {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let millis = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
    // SAFETY: `pfd` is a valid, initialised pollfd that outlives the call, and
    // the count matches the one entry passed.
    let ready = unsafe { libc::poll(&mut pfd, 1, millis) };
    ready > 0 && pfd.revents & (libc::POLLIN | libc::POLLHUP) != 0
}

/// Reads the pty until EOF, handing `emit` one frame per burst of output.
///
/// Each frame merges every read that `more_soon` reports as already waiting,
/// up to `MAX_FRAME_BYTES` or `COALESCE_WINDOW` after the first read. Reads
/// land on arbitrary byte boundaries, so a multi-byte character can straddle
/// two of them; frames are rendered with
/// `from_utf8_lossy`, which would turn the halves into permanent replacement
/// characters, so an incomplete tail is held back until the next read
/// completes it. Stops early once `emit` returns false.
fn pump(
    reader: &mut dyn Read,
    mut more_soon: impl FnMut(Duration) -> bool,
    mut emit: impl FnMut(Vec<u8>) -> bool,
) {
    let mut buf = [0u8; 8192];
    let mut carry: Vec<u8> = Vec::new();
    let mut eof = false;
    while !eof {
        let mut chunk = std::mem::take(&mut carry);
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => chunk.extend_from_slice(&buf[..n]),
        }
        let deadline = Instant::now() + COALESCE_WINDOW;
        while chunk.len() < MAX_FRAME_BYTES {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() || !more_soon(remaining) {
                break;
            }
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    eof = true;
                    break;
                }
                Ok(n) => chunk.extend_from_slice(&buf[..n]),
            }
        }
        carry = chunk.split_off(utf8_prefix_len(&chunk));
        if !chunk.is_empty() && !emit(chunk) {
            return;
        }
    }
    if !carry.is_empty() {
        emit(carry);
    }
}

/// Length of the longest prefix of `bytes` that ends on a UTF-8 character
/// boundary. Only the last three bytes can start an incomplete sequence.
fn utf8_prefix_len(bytes: &[u8]) -> usize {
    let len = bytes.len();
    for i in (len.saturating_sub(3)..len).rev() {
        let b = bytes[i];
        if b & 0b1100_0000 == 0b1000_0000 {
            continue; // continuation byte; keep scanning back for the lead
        }
        let need = if b < 0x80 {
            1
        } else if b >> 5 == 0b110 {
            2
        } else if b >> 4 == 0b1110 {
            3
        } else if b >> 3 == 0b1_1110 {
            4
        } else {
            1 // invalid lead: pass it through and let the decoder replace it
        };
        return if i + need <= len { len } else { i };
    }
    len
}

struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

impl RuntimeSession for PtySession {
    fn send_input(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.writer.write_all(bytes)?;
        self.writer.flush()?;
        Ok(())
    }

    fn resize(&mut self, cols: u16, rows: u16) -> anyhow::Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    fn kill(&mut self) -> anyhow::Result<()> {
        self.killer.kill()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(out.len(), 2, "the cap ends the frame before it overflows");
        assert_eq!(out[0].len(), MAX_FRAME_BYTES + 1);
        assert_eq!(out[1], b"z");
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
}
