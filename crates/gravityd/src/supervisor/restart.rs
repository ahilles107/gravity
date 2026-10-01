use super::*;

impl BotHandle {
    pub(super) fn prepare_terminal(&mut self, runtime: bus::BotRuntime) {
        if self
            .terminal_runtime
            .is_some_and(|previous| previous != runtime)
        {
            self.term.push(b"\x1bc".to_vec());
        }
        self.terminal_runtime = Some(runtime);
    }
}

impl Supervisor {
    pub(super) fn report_start_failure(&self, bot_id: &str, error: &anyhow::Error) {
        let (state, _) = self.state(bot_id);
        let reason = format!("Runtime failed to start: {error:#}");
        self.set_state(bot_id, BotState::Crashed, &reason);
        if state != BotState::Crashed {
            if let Some(term) = self.term(bot_id) {
                term.push(format!("\r\n[Gravity] {reason}\r\n").into_bytes());
            }
            self.inner
                .events
                .push(Push::notice("error", "Bot could not start", reason));
        }
    }
    pub fn restart_bot(&self, bot_id: &str) -> anyhow::Result<()> {
        let previous = self.state(bot_id);
        let session = {
            let mut bots = self.lock_bots();
            let Some(handle) = bots.get_mut(bot_id) else {
                return Ok(());
            };
            handle.restart_pending = handle.session.is_some();
            handle.stopping = handle.session.is_some();
            handle.next_start_at = None;
            handle.session.clone()
        };
        if let Some(session) = session {
            self.set_state(bot_id, BotState::Stopping, "runtime changed");
            let result = session.lock().unwrap_or_else(|e| e.into_inner()).kill();
            if let Err(error) = result {
                if let Some(handle) = self.lock_bots().get_mut(bot_id) {
                    handle.stopping = false;
                    handle.restart_pending = false;
                }
                self.set_state(bot_id, previous.0, &previous.1);
                return Err(error);
            }
        }
        Ok(())
    }
}
