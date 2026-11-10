//! The Win32 message pump, which also delivers WinEvents and runs the tray's window.

use std::time::Instant;

use thiserror::Error;
use windows::Win32::Foundation::{LPARAM, WAIT_FAILED, WPARAM};
use windows::Win32::System::Threading::{GetCurrentThreadId, INFINITE};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE,
    PeekMessageW, PostThreadMessageW, QS_ALLINPUT, TranslateMessage, WM_APP,
};

/// Posted to the main thread only to end its wait; the payload is already queued.
const WM_WAKE: u32 = WM_APP + 1;

#[derive(Debug, Error)]
#[error("the main thread's message queue could not be set up: {0}")]
pub struct LoopError(#[source] windows::core::Error);

#[derive(Debug, Clone)]
pub struct Waker {
    thread_id: u32,
}

impl Waker {
    pub fn wake(&self) {
        // a wake posted while a menu runs its own modal loop is dropped, which is fine:
        // the main loop drains its queues as soon as that menu returns control
        // SAFETY: a parameterless thread message carries no pointers.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_WAKE, WPARAM(0), LPARAM(0)) };
    }
}

#[derive(Debug)]
pub struct Loop {
    _not_send: std::marker::PhantomData<*const ()>,
}

impl Loop {
    pub fn new() -> Result<(Self, Waker), LoopError> {
        let mut message = MSG::default();

        // SAFETY: peeking forces the thread's queue into existence, so a wake posted before
        // the first wait is not lost; the pointer targets a live local.
        let _ = unsafe { PeekMessageW(&mut message, None, 0, 0, Default::default()) };

        // SAFETY: reads this thread's own id, no pointers involved.
        let thread_id = unsafe { GetCurrentThreadId() };
        if thread_id == 0 {
            return Err(LoopError(windows::core::Error::from_thread()));
        }

        Ok((
            Self {
                _not_send: std::marker::PhantomData,
            },
            Waker { thread_id },
        ))
    }

    pub fn wait(&mut self, deadline: Option<Instant>) {
        if dispatch_pending() {
            return;
        }

        let timeout_ms = match deadline {
            Some(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                u32::try_from(left.as_millis()).unwrap_or(INFINITE - 1)
            }
            None => INFINITE,
        };

        // SAFETY: no handles are passed, so this only waits on the message queue.
        let woke = unsafe {
            MsgWaitForMultipleObjectsEx(None, timeout_ms, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        };
        if woke == WAIT_FAILED {
            return;
        }
        dispatch_pending();
    }
}

/// Reports whether anything was waiting.
fn dispatch_pending() -> bool {
    let mut message = MSG::default();
    let mut any = false;

    // SAFETY: the pointer targets a live local for each call.
    while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
        any = true;
        if message.message == WM_WAKE && message.hwnd.is_invalid() {
            continue;
        }
        // SAFETY: the message was just filled in by PeekMessageW.
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    any
}
