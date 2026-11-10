//! The extension says when the window list changed; the whole list is then fetched again
//! and handed to the model, which keeps apps in the order they appeared.

use std::sync::{Arc, Mutex, MutexGuard};

use tracing::warn;

use super::{apps, shell};
use crate::event_loop::Waker;
use crate::watcher::{WatchError, WindowEvent};

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

/// The signal thread blocks on the bus for the process lifetime, so it is left to end
/// with the process rather than joined.
#[derive(Debug)]
pub struct Watcher {
    pending: Arc<Pending>,
}

impl Watcher {
    pub fn install(waker: Waker) -> Result<Self, WatchError> {
        shell::check().map_err(WatchError)?;
        let changes = shell::proxy()
            .and_then(|proxy| {
                proxy
                    .receive_signal("WindowsChanged")
                    .map_err(|error| format!("could not subscribe to window changes: {error}"))
            })
            .map_err(WatchError)?;

        let pending = Arc::new(Pending::default());
        let queue = Arc::clone(&pending);
        std::thread::Builder::new()
            .name("window-watch".to_owned())
            .spawn(move || {
                for _ in changes {
                    snapshot_into(&queue, &waker);
                }
            })
            .map_err(|error| WatchError(format!("could not start the window watcher: {error}")))?;

        Ok(Self { pending })
    }

    pub fn drain(&self) -> Vec<WindowEvent> {
        std::mem::take(&mut *self.pending.events())
    }
}

fn snapshot_into(pending: &Pending, waker: &Waker) {
    let (windows, focused) = match apps::enumerate() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            warn!("the running app list is stale: {error}");
            return;
        }
    };

    // only the newest list matters, so an unread older one is replaced
    let mut events = pending.events();
    events.clear();
    events.push(WindowEvent::Snapshot { windows, focused });
    drop(events);
    waker.wake();
}
