//! Nothing on Linux needs the main thread pumped: the shell, the tray and input all live on
//! threads of their own. Waiting is just a channel with a timeout.

use std::convert::Infallible;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::time::Instant;

use thiserror::Error;

/// One pending wake is enough; more would only make the loop spin through empty queues.
const WAKE_SLOTS: usize = 1;

#[derive(Debug, Error)]
#[error("unreachable: setting up the loop cannot fail on Linux")]
pub struct LoopError(Infallible);

#[derive(Debug, Clone)]
pub struct Waker {
    wake: SyncSender<()>,
}

impl Waker {
    pub fn wake(&self) {
        match self.wake.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) | Err(TrySendError::Disconnected(())) => {}
        }
    }
}

#[derive(Debug)]
pub struct Loop {
    woken: Receiver<()>,
}

impl Loop {
    pub fn new() -> Result<(Self, Waker), LoopError> {
        let (wake, woken) = mpsc::sync_channel(WAKE_SLOTS);
        Ok((Self { woken }, Waker { wake }))
    }

    pub fn wait(&mut self, deadline: Option<Instant>) {
        let Some(deadline) = deadline else {
            let _ = self.woken.recv();
            return;
        };

        let left = deadline.saturating_duration_since(Instant::now());
        match self.woken.recv_timeout(left) {
            Ok(()) | Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_wake_ends_the_wait() {
        let (mut waiting, waker) = Loop::new().expect("a loop");
        waker.wake();
        waker.wake();

        let started = Instant::now();
        waiting.wait(Some(Instant::now() + Duration::from_secs(5)));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_deadline_ends_the_wait_without_a_wake() {
        let (mut waiting, _waker) = Loop::new().expect("a loop");
        let deadline = Instant::now() + Duration::from_millis(20);

        waiting.wait(Some(deadline));
        assert!(Instant::now() >= deadline);
    }
}
