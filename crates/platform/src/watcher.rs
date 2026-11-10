//! Keeps the window model current, so opening the wheel never has to enumerate.

use thiserror::Error;

use crate::apps::{WindowId, WindowInfo};
use crate::event_loop::Waker;
use crate::sys;

/// What happened to a window, in model terms.
///
/// Windows reports single windows; GNOME and macOS hand over the whole list, which the
/// model merges so apps keep their order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowEvent {
    /// Re-check the window: it may have become, or stopped being, a taskbar entry.
    Changed(WindowId),
    /// The window is gone, so it cannot be queried any more.
    Vanished(WindowId),
    Focused(WindowId),
    Snapshot {
        windows: Vec<WindowInfo>,
        focused: Option<WindowId>,
    },
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct WatchError(pub(crate) String);

/// Stops watching on drop.
#[derive(Debug)]
pub struct Watcher {
    sys: sys::watcher::Watcher,
}

impl Watcher {
    /// Must be called on the main thread, which is the one that drains.
    pub fn install(waker: Waker) -> Result<Self, WatchError> {
        Ok(Self {
            sys: sys::watcher::Watcher::install(waker)?,
        })
    }

    /// Takes the events seen since the last call.
    pub fn drain(&self) -> Vec<WindowEvent> {
        self.sys.drain()
    }
}
