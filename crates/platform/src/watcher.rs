//! WinEvent subscriptions that keep the window model current, so opening the wheel
//! never has to enumerate.

use std::cell::RefCell;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, EVENT_OBJECT_CLOAKED, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY,
    EVENT_OBJECT_HIDE, EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_SHOW, EVENT_OBJECT_UNCLOAKED,
    EVENT_SYSTEM_FOREGROUND, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};
use windows::core::Result;

/// What happened to a window, in model terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowEvent {
    /// Re-check the window: it may have become, or stopped being, a taskbar entry.
    Changed(HWND),
    /// The handle is gone, so it cannot be queried any more.
    Vanished(HWND),
    Focused(HWND),
}

/// Event ranges worth subscribing to, as `(first, last)` pairs.
///
/// Location and value changes are deliberately absent: they fire constantly during
/// window drags and tell us nothing about which apps are running.
const SUBSCRIPTIONS: &[(u32, u32)] = &[
    (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
    (EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE),
    (EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE),
    (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
];

thread_local! {
    /// Filled by the callback, taken by `drain`. Thread-local because out-of-context
    /// WinEvents are delivered on the thread that pumps messages, which also drains.
    static PENDING: RefCell<Vec<WindowEvent>> = const { RefCell::new(Vec::new()) };
}

/// Drops the subscriptions on drop.
#[derive(Debug)]
pub struct Watcher {
    hooks: Vec<HWINEVENTHOOK>,
}

impl Watcher {
    /// Subscribes on the calling thread, which must be the one that pumps messages.
    pub fn install() -> Result<Self> {
        let mut hooks = Vec::with_capacity(SUBSCRIPTIONS.len());

        for (first, last) in SUBSCRIPTIONS {
            // SAFETY: the callback is static and the flags keep it out of other
            // processes, so nothing is injected anywhere.
            let hook = unsafe {
                SetWinEventHook(
                    *first,
                    *last,
                    None,
                    Some(on_win_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                )
            };

            if hook.is_invalid() {
                return Err(windows::core::Error::from_thread());
            }
            hooks.push(hook);
        }

        Ok(Self { hooks })
    }

    /// Takes the events seen since the last call.
    pub fn drain(&self) -> Vec<WindowEvent> {
        PENDING.with_borrow_mut(std::mem::take)
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        for hook in self.hooks.drain(..) {
            // SAFETY: every handle came from SetWinEventHook on this thread.
            let _ = unsafe { UnhookWinEvent(hook) };
        }
    }
}

unsafe extern "system" fn on_win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    window: HWND,
    object_id: i32,
    child_id: i32,
    _thread_id: u32,
    _timestamp: u32,
) {
    // only whole-window events matter; the rest is caret, menu and scrollbar noise
    if object_id != OBJID_WINDOW.0 || child_id != CHILDID_SELF as i32 {
        return;
    }

    let Some(change) = classify(event, window) else {
        return;
    };
    PENDING.with_borrow_mut(|pending| pending.push(change));
}

/// Everything except a destroy becomes `Changed`, so one predicate in the model
/// decides whether the window belongs rather than each event guessing.
fn classify(event: u32, window: HWND) -> Option<WindowEvent> {
    match event {
        EVENT_SYSTEM_FOREGROUND => Some(WindowEvent::Focused(window)),
        EVENT_OBJECT_DESTROY => Some(WindowEvent::Vanished(window)),
        EVENT_OBJECT_CREATE
        | EVENT_OBJECT_SHOW
        | EVENT_OBJECT_HIDE
        | EVENT_OBJECT_NAMECHANGE
        | EVENT_OBJECT_CLOAKED
        | EVENT_OBJECT_UNCLOAKED => Some(WindowEvent::Changed(window)),
        _ => None,
    }
}
