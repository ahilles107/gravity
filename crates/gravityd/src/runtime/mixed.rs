use super::codex::CodexAdapter;
use super::pty::PtyAdapter;
use super::{BotSpec, Capabilities, RuntimeAdapter, StartedSession};
use crate::config::Config;

pub struct MixedAdapter {
    claude_bin: String,
    codex_bin: String,
}

impl MixedAdapter {
    pub fn new(cfg: &Config) -> Self {
        Self {
            claude_bin: cfg.claude_bin.clone(),
            codex_bin: cfg.codex_bin.clone(),
        }
    }
}

impl RuntimeAdapter for MixedAdapter {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            kind: "cli",
            native_background: false,
            channel_delivery: true,
            permission_relay: true,
        }
    }
    fn start(&self, spec: &BotSpec) -> anyhow::Result<StartedSession> {
        if spec.codex.is_some() {
            CodexAdapter.start(spec)
        } else {
            PtyAdapter.start(spec)
        }
    }
    fn probe(&self) -> anyhow::Result<String> {
        let versions: Vec<String> = [&self.claude_bin, &self.codex_bin]
            .into_iter()
            .filter_map(|bin| {
                std::process::Command::new(bin)
                    .arg("--version")
                    .output()
                    .ok()
                    .filter(|out| out.status.success())
                    .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            })
            .collect();
        anyhow::ensure!(!versions.is_empty(), "neither configured CLI is available");
        Ok(versions.join("; "))
    }
}
