//! macOS has no single "a window changed" notification, so the window list is read again
//! on a timer, off the main thread, and only handed over when it differs.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use objc2::rc::autoreleasepool;
use tracing::debug;

use super::apps;
use crate::apps::{WindowId, WindowInfo};
use crate::event_loop::Waker;
use crate::watcher::{WatchError, WindowEvent};

/// Fast enough that a just-opened app is on the wheel by the time you reach for it.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Default)]
struct Pending {
    events: Mutex<Vec<WindowEvent>>,
}

impl Pending {
    fn events(&self) -> MutexGuard<'_, Vec<WindowEvent>> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The polling thread runs for the process lifetime.
#[derive(Debug)]
pub struct Watcher {
    pending: Arc<Pending>,
}

impl Watcher {
    pub fn install(waker: Waker) -> Result<Self, WatchError> {
        apps::enumerate().map_err(|error| WatchError(error.to_string()))?;

        let pending = Arc::new(Pending::default());
        let queue = Arc::clone(&pending);
        std::thread::Builder::new()
            .name("window-poll".to_owned())
            .spawn(move || poll(&queue, &waker))
            .map_err(|error| WatchError(format!("could not start the window poller: {error}")))?;

        Ok(Self { pending })
    }

    pub fn drain(&self) -> Vec<WindowEvent> {
        std::mem::take(&mut *self.pending.events())
    }
}

fn poll(pending: &Pending, waker: &Waker) {
    let mut last: Option<(Vec<WindowInfo>, Option<WindowId>)> = None;
    loop {
        std::thread::sleep(POLL_INTERVAL);
        let snapshot = autoreleasepool(|_| apps::enumerate());
        let snapshot = match snapshot {
            Ok(snapshot) => snapshot,
            Err(error) => {
                debug!("window poll skipped: {error}");
                continue;
            }
        };
        if last.as_ref() == Some(&snapshot) {
            continue;
        }

        let (windows, focused) = snapshot.clone();
        let mut events = pending.events();
        events.clear();
        events.push(WindowEvent::Snapshot { windows, focused });
        drop(events);
        last = Some(snapshot);
        waker.wake();
    }
}
