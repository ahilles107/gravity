use super::codex::NativeCodexAdapter;
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
    fn check_available(&self, runtime: bus::BotRuntime) -> anyhow::Result<()> {
        let (bin, setting) = match runtime {
            bus::BotRuntime::ClaudeCode => (&self.claude_bin, "claude_bin"),
            bus::BotRuntime::CodexCli => (&self.codex_bin, "codex_bin"),
        };
        super::executable::version(bin).map(|_| ()).map_err(|error| {
            anyhow::anyhow!("{error:#}. Install the CLI or set {setting} to its executable path in gravityd.toml.")
        })
    }
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
            NativeCodexAdapter.start(spec)
        } else {
            PtyAdapter.start(spec)
        }
    }
    fn probe(&self) -> anyhow::Result<String> {
        let versions: Vec<String> = [&self.claude_bin, &self.codex_bin]
            .into_iter()
            .filter_map(|bin| super::executable::version(bin).ok())
            .collect();
        anyhow::ensure!(!versions.is_empty(), "neither configured CLI is available");
        Ok(versions.join("; "))
    }
}
