use super::*;

impl Supervisor {
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
