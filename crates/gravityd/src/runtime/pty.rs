//! PTY runtime adapter: spawns `claude` inside a portable-pty and streams
//! terminal output. The PTY is strictly for the user's terminal — deliveries
//! go through the session's inbox socket, reported by the SessionStart hook.

use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::RawFd;
use std::process::Command as StdCommand;
use std::time::{Duration, Instant};

use anyhow::Context;
#[cfg(unix)]
use portable_pty::ChildKiller;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
#[cfg(windows)]
mod windows;
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
            if key == "CODEX_THREAD_ID" || key == "CLAUDECODE" || key.starts_with("CLAUDE_CODE_") {
                cmd.env_remove(&key);
            }
        }
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        cmd.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(cmd)
            .context("spawn terminal CLI")?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().context("clone pty reader")?;
        let writer = pair.master.take_writer().context("take pty writer")?;
        #[cfg(unix)]
        let master_fd = pair.master.as_raw_fd();

        let (tx, rx) = mpsc::unbounded_channel();
        #[cfg(unix)]
        let killer = child.clone_killer();
        #[cfg(windows)]
        let killer = windows::ProcessKiller::new(child.as_ref())?;

        // ConPTY keeps its read pipe open until the master is dropped. Waiting
        // for reader EOF first would hide exits forever, preventing restarts.
        #[cfg(windows)]
        {
            let exit_tx = tx.clone();
            std::thread::spawn(move || {
                let mut child = child;
                let code = child.wait().ok().map(|s| s.exit_code() as i32);
                // Allow the console host to flush its final screen update.
                std::thread::sleep(Duration::from_millis(50));
                let _ = exit_tx.send(SessionEvent::Exited { code });
            });
        }

        // Blocking reader thread; ends when the child exits and the pty EOFs.
        let out_tx = tx.clone();
        std::thread::spawn(move || {
            #[cfg(unix)]
            let mut child = child;
            #[cfg(unix)]
            let more_soon = |timeout| master_fd.is_some_and(|fd| readable_within(fd, timeout));
            // ConPTY exposes a blocking pipe rather than a pollable Unix fd.
            #[cfg(windows)]
            let more_soon = |_timeout| false;
            pump(&mut reader, more_soon, |chunk| {
                out_tx.send(SessionEvent::Output(chunk)).is_ok()
            });
            #[cfg(unix)]
            {
                let code = child.wait().ok().map(|s| s.exit_code() as i32);
                let _ = out_tx.send(SessionEvent::Exited { code });
            }
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
#[cfg(unix)]
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
    #[cfg(unix)]
    killer: Box<dyn ChildKiller + Send + Sync>,
    #[cfg(windows)]
    killer: windows::ProcessKiller,
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
mod tests;
