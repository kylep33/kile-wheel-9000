//! The main thread's loop.
//!
//! Hooks, watchers, the tray and the config watcher all run on threads or callbacks of
//! their own. They queue what happened and wake this loop, so the app's state is only
//! ever touched from the main thread.

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use crate::sys;
use crate::tray::Command;

/// Something a background source wants the main thread to act on. Input and window
/// changes have queues of their own and only wake the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// A second launch asked the running instance to open its wheel.
    OpenWheel,
    Tray(Command),
    ConfigChanged,
}

/// Cheap to clone and safe to use from any thread.
#[derive(Debug, Clone)]
pub struct Waker {
    notices: Sender<Notice>,
    sys: sys::event_loop::Waker,
}

impl Waker {
    /// Makes `wait` return so the main thread drains its queues.
    pub fn wake(&self) {
        self.sys.wake();
    }

    pub fn send(&self, notice: Notice) {
        // the loop only goes away on quit, when nothing is left to act on the notice
        if self.notices.send(notice).is_ok() {
            self.wake();
        }
    }
}

#[derive(Debug)]
pub struct EventLoop {
    notices: Receiver<Notice>,
    waker: Waker,
    sys: sys::event_loop::Loop,
}

impl EventLoop {
    /// Must be called on the main thread, which is the one that then calls `wait`.
    pub fn new() -> Result<Self, sys::event_loop::LoopError> {
        let (sender, notices) = mpsc::channel();
        let (sys, sys_waker) = sys::event_loop::Loop::new()?;

        Ok(Self {
            notices,
            waker: Waker {
                notices: sender,
                sys: sys_waker,
            },
            sys,
        })
    }

    pub fn waker(&self) -> Waker {
        self.waker.clone()
    }

    /// Dispatches the OS's own events, then blocks until a wake, another OS event, or the
    /// deadline, whichever is first. `None` waits for as long as it takes.
    pub fn wait(&mut self, deadline: Option<Instant>) {
        self.sys.wait(deadline);
    }

    pub fn take_notices(&self) -> Vec<Notice> {
        self.notices.try_iter().collect()
    }
}
