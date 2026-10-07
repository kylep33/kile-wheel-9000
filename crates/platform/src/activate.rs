//! Bringing a window forward.

use thiserror::Error;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, SW_MINIMIZE,
    SW_RESTORE, SetForegroundWindow, ShowWindow,
};

#[derive(Debug, Error)]
pub enum ActivateError {
    #[error(
        "SetForegroundWindow was refused for window {0:#x}, so it stayed where it was; another process is holding the foreground lock"
    )]
    ForegroundRefused(isize),
}

pub fn is_minimized(window: HWND) -> bool {
    // SAFETY: a stale handle reports not-minimized rather than misbehaving.
    unsafe { IsIconic(window) }.as_bool()
}

pub fn is_foreground(window: HWND) -> bool {
    // SAFETY: returns a borrowed handle or null, no pointers involved.
    (unsafe { GetForegroundWindow() }) == window
}

pub fn foreground_window() -> Option<HWND> {
    // SAFETY: returns a borrowed handle or null, no pointers involved.
    let window = unsafe { GetForegroundWindow() };
    (!window.is_invalid()).then_some(window)
}

pub fn minimize(window: HWND) {
    // SAFETY: a stale handle makes this a no-op.
    let _ = unsafe { ShowWindow(window, SW_MINIMIZE) };
}

/// Brings the window forward, restoring it first if it is minimized.
///
/// A background process cannot normally take the foreground. The way through is to
/// borrow the input queue of the thread that currently *owns* the foreground, not the
/// target's own thread: that is the thread holding the lock Windows checks.
pub fn focus(window: HWND) -> Result<(), ActivateError> {
    if is_minimized(window) {
        // SAFETY: a stale handle makes this a no-op.
        let _ = unsafe { ShowWindow(window, SW_RESTORE) };
    }

    let borrowed = foreground_window()
        .filter(|foreground| *foreground != window)
        .and_then(attach_to);

    // SAFETY: a stale handle makes these fail rather than misbehave.
    let raised = unsafe {
        let raised = SetForegroundWindow(window).as_bool();
        let _ = BringWindowToTop(window);
        let _ = SetFocus(Some(window));
        raised
    };

    if let Some(thread) = borrowed {
        detach_from(thread);
    }

    if raised {
        return Ok(());
    }
    Err(ActivateError::ForegroundRefused(window.0 as isize))
}

/// Borrows the given window's thread input queue. Returns the thread it attached to.
fn attach_to(window: HWND) -> Option<u32> {
    // SAFETY: reads the owning thread id; passing None skips the process id.
    let target = unsafe { GetWindowThreadProcessId(window, None) };
    if target == 0 {
        return None;
    }

    let ours = current_thread();
    if ours == target {
        return None;
    }

    // SAFETY: detached again by detach_from before focus() returns.
    if !unsafe { AttachThreadInput(ours, target, true) }.as_bool() {
        return None;
    }
    Some(target)
}

fn detach_from(target: u32) {
    // SAFETY: undoes exactly the attach that attach_to made.
    let _ = unsafe { AttachThreadInput(current_thread(), target, false) };
}

fn current_thread() -> u32 {
    // SAFETY: reads this thread's own id, no pointers involved.
    unsafe { GetCurrentThreadId() }
}
