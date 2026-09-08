//! Runtime-writable configuration, layered over the immutable [`Config`].
//!
//! `Config` is cloned by value into the app state and the supervisor at boot,
//! so a value the control plane can change at runtime needs a shared handle
//! instead. The override is persisted in the `meta` table and re-applied on
//! startup, which keeps the user's hand-written `gravityd.toml` untouched.

use std::sync::{Arc, Mutex};

use crate::config::{Config, AUTO_COMPACT_WINDOW_MAX, AUTO_COMPACT_WINDOW_MIN};
use crate::db::Db;

/// The `meta` table key the override is persisted under.
pub const AUTO_COMPACT_META_KEY: &str = "auto_compact_window";
/// The `meta` value meaning "explicitly use the model default (no env var)".
pub const AUTO_COMPACT_META_DEFAULT: &str = "default";

/// The `auto_compact_window` value `set_config` wrote, if any.
///
/// The outer `Option` is "was an override ever set"; the inner one mirrors the
/// config field, where `None` means "leave the model default in place".
#[derive(Clone, Default)]
pub struct AutoCompactOverride {
    value: Arc<Mutex<Option<Option<u32>>>>,
}

impl AutoCompactOverride {
    pub fn set(&self, window: Option<u32>) {
        *self.value.lock().unwrap_or_else(|e| e.into_inner()) = Some(window);
    }

    fn get(&self) -> Option<Option<u32>> {
        *self.value.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Persists and publishes an override as one serialized operation so two
    /// control connections cannot leave the database and runtime disagreeing.
    pub fn persist(&self, db: &Db, window: Option<u32>) -> anyhow::Result<()> {
        let mut value = self.value.lock().unwrap_or_else(|e| e.into_inner());
        db.set_meta(AUTO_COMPACT_META_KEY, &auto_compact_meta_value(window))?;
        *value = Some(window);
        Ok(())
    }

    /// The window bots are spawned with: the override when one was set, the
    /// config file's value otherwise, clamped either way.
    pub fn effective(&self, cfg: &Config) -> Option<u32> {
        match self.get() {
            Some(window) => {
                window.map(|w| w.clamp(AUTO_COMPACT_WINDOW_MIN, AUTO_COMPACT_WINDOW_MAX))
            }
            None => cfg.effective_auto_compact_window(),
        }
    }

    /// Restores the persisted override; unknown values are ignored so a
    /// corrupt row cannot keep the daemon from booting.
    pub fn restore(&self, stored: &str) {
        if stored == AUTO_COMPACT_META_DEFAULT {
            self.set(None);
        } else if let Ok(window) = stored.parse::<u32>() {
            self.set(Some(window));
        }
    }
}

/// How the override is written to the `meta` table.
pub fn auto_compact_meta_value(window: Option<u32>) -> String {
    match window {
        Some(w) => w.to_string(),
        None => AUTO_COMPACT_META_DEFAULT.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_the_config_until_set() {
        let over = AutoCompactOverride::default();
        let cfg = Config::default();
        assert_eq!(over.effective(&cfg), cfg.effective_auto_compact_window());

        over.set(Some(300_000));
        assert_eq!(over.effective(&cfg), Some(300_000));

        over.set(None);
        assert_eq!(over.effective(&cfg), None);
    }

    #[test]
    fn clamps_like_the_config_does() {
        let over = AutoCompactOverride::default();
        over.set(Some(1));
        assert_eq!(over.effective(&Config::default()), Some(100_000));
    }

    #[test]
    fn restores_persisted_values_and_ignores_garbage() {
        let cfg = Config::default();

        let over = AutoCompactOverride::default();
        over.restore("300000");
        assert_eq!(over.effective(&cfg), Some(300_000));

        let cleared = AutoCompactOverride::default();
        cleared.restore(AUTO_COMPACT_META_DEFAULT);
        assert_eq!(cleared.effective(&cfg), None);

        let garbage = AutoCompactOverride::default();
        garbage.restore("not-a-number");
        assert_eq!(garbage.effective(&cfg), cfg.effective_auto_compact_window());
    }

    #[test]
    fn meta_value_round_trips() {
        assert_eq!(auto_compact_meta_value(Some(300_000)), "300000");
        assert_eq!(auto_compact_meta_value(None), AUTO_COMPACT_META_DEFAULT);
    }
}
