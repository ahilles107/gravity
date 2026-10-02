//! One live stream per bot browser, shared by everyone watching it: the
//! desktop app and a phone looking at the same bot read the same screencast
//! rather than each attaching their own.
//!
//! Each stream holds only the latest value (tokio `watch` channels): a viewer
//! that joins sees the current tabs and screen at once, and a slow one (a
//! phone on a weak link) skips to the newest frame instead of falling behind.
//! Streams live while someone watches: the last viewer leaving drops them, and
//! dropping one stops its polling or its screencast.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::cdp::{self, Tab};

/// How often a browser's tab list is checked: tabs opening, closing,
/// navigating.
const POLL: Duration = Duration::from_secs(1);

/// One screen of a tab, a base64 JPEG.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub data: Arc<str>,
    pub width: u64,
    pub height: u64,
}

/// A task that stops when its owner is dropped.
struct Owned(JoinHandle<()>);

impl Drop for Owned {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// A bot browser's tabs, kept current while anyone watches.
pub struct BotStream {
    /// `None` until the first look; then the open tabs, most recently used
    /// first (empty while the browser is closed).
    tabs: watch::Receiver<Option<Vec<Tab>>>,
    screens: Mutex<HashMap<String, Weak<TabStream>>>,
    _poller: Owned,
}

/// One tab's screen, kept current while anyone watches it.
pub struct TabStream {
    frames: watch::Receiver<Option<Frame>>,
    _screencast: Owned,
}

impl TabStream {
    /// The tab's screens from now on, starting with the current one.
    pub fn frames(&self) -> watch::Receiver<Option<Frame>> {
        let mut frames = self.frames.clone();
        frames.mark_changed();
        frames
    }
}

impl BotStream {
    /// The browser's tabs from now on, starting with the current list.
    pub fn tabs(&self) -> watch::Receiver<Option<Vec<Tab>>> {
        let mut tabs = self.tabs.clone();
        tabs.mark_changed();
        tabs
    }

    /// The shared screencast of one tab, started for the first viewer.
    pub fn screen(&self, tab: &Tab) -> Arc<TabStream> {
        let mut screens = self.screens.lock().unwrap_or_else(|e| e.into_inner());
        screens.retain(|_, stream| stream.strong_count() > 0);
        if let Some(stream) = screens.get(&tab.id).and_then(Weak::upgrade) {
            return stream;
        }
        let (send, frames) = watch::channel(None);
        let target = tab.clone();
        let task = tokio::spawn(async move {
            let result = cdp::screencast(&target, |data, width, height| {
                send.send_replace(Some(Frame {
                    data: data.into(),
                    width,
                    height,
                }));
                true
            })
            .await;
            if let Err(e) = result {
                tracing::debug!(error = %e, "browser screencast ended");
            }
        });
        let stream = Arc::new(TabStream {
            frames,
            _screencast: Owned(task),
        });
        screens.insert(tab.id.clone(), Arc::downgrade(&stream));
        stream
    }
}

/// Every bot browser someone is watching.
#[derive(Default)]
pub struct BrowserStreams {
    bots: Mutex<HashMap<String, Weak<BotStream>>>,
}

impl BrowserStreams {
    /// The shared stream of the browser running on `profile`, started for the
    /// first viewer of `bot_id`.
    pub fn bot(&self, bot_id: &str, profile: PathBuf) -> Arc<BotStream> {
        let mut bots = self.bots.lock().unwrap_or_else(|e| e.into_inner());
        bots.retain(|_, stream| stream.strong_count() > 0);
        if let Some(stream) = bots.get(bot_id).and_then(Weak::upgrade) {
            return stream;
        }
        let (send, tabs) = watch::channel(None);
        let task = tokio::spawn(async move {
            loop {
                let open = match cdp::port(&profile) {
                    Some(port) => cdp::tabs(port).await.unwrap_or_default(),
                    None => Vec::new(),
                };
                send.send_if_modified(|seen| {
                    let changed = seen.as_ref() != Some(&open);
                    *seen = Some(open);
                    changed
                });
                tokio::time::sleep(POLL).await;
            }
        });
        let stream = Arc::new(BotStream {
            tabs,
            screens: Mutex::default(),
            _poller: Owned(task),
        });
        bots.insert(bot_id.to_string(), Arc::downgrade(&stream));
        stream
    }

    /// How many bot browsers are being streamed, for tests and diagnostics.
    pub fn live(&self) -> usize {
        let bots = self.bots.lock().unwrap_or_else(|e| e.into_inner());
        bots.values()
            .filter(|stream| stream.strong_count() > 0)
            .count()
    }
}
