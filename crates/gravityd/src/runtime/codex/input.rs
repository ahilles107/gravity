//! Legacy headless input used by protocol tests. Native bots use the real TUI.
use super::worker::Worker;
use crate::runtime::SessionEvent;
use anyhow::Context;
use serde_json::json;

impl Worker {
    pub fn keystrokes(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        for &byte in bytes {
            if self.escaping {
                if byte != b'[' && (0x40..=0x7e).contains(&byte) {
                    self.escaping = false;
                }
                continue;
            }
            match byte {
                27 => self.escaping = true,
                3 => {
                    self.line.clear();
                    self.interrupt()?;
                }
                8 | 127 => {
                    if let Some(last) = self.line.pop() {
                        if last & 0xc0 == 0x80 {
                            while self.line.pop().is_some_and(|b| b & 0xc0 == 0x80) {}
                        }
                        self.output("\u{8} \u{8}");
                    }
                }
                13 | 10 if !self.line.is_empty() => {
                    let text = String::from_utf8(std::mem::take(&mut self.line))
                        .context("prompt is not UTF-8")?;
                    self.output("\n");
                    if text == "/interrupt" {
                        self.interrupt()?;
                    } else if !self.answer(&text)? {
                        self.deliver(&text)?;
                    }
                }
                13 | 10 => {}
                b if b >= 32 => self.line.push(b),
                _ => {}
            }
        }
        // Echo whole UTF-8 inputs rather than one byte per output frame.
        if !bytes
            .iter()
            .any(|b| matches!(b, 3 | 8 | 10 | 13 | 27 | 127))
        {
            let _ = self.events.send(SessionEvent::Output(bytes.to_vec()));
        }
        Ok(())
    }
    fn interrupt(&mut self) -> anyhow::Result<()> {
        if let Some(turn) = &self.turn {
            self.rpc(
                "turn/interrupt",
                json!({ "threadId": self.thread, "turnId": turn }),
            )?;
        }
        Ok(())
    }
}
