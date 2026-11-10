//! Low-level keyboard and mouse hooks, on a thread of their own.
//!
//! The hooks do not share the main thread because Windows silently unhooks a callback
//! that overruns `LowLevelHooksTimeout`, and the main thread also renders.

use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;

use kile_wheel_core::binding::{KeyCode, Modifiers, MouseButton};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1,
};

use super::send::OWN_INPUT_TAG;
use crate::input::{self, InstallError, SharedInput};

/// Returned from a hook callback to stop the event reaching the focused app.
const SWALLOW: LRESULT = LRESULT(1);

/// `GetAsyncKeyState` sets the high bit while a key is physically down.
const KEY_PRESSED_MASK: u16 = 0x8000;

/// The hook callbacks have no user-data parameter, so the state they read is global.
static SHARED: OnceLock<Arc<SharedInput>> = OnceLock::new();

/// Stops the hook thread and removes the hooks on drop.
#[derive(Debug)]
pub struct Capture {
    thread_id: u32,
    worker: Option<JoinHandle<()>>,
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: posting to a thread that already exited fails rather than misbehaves.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Capture {
    pub fn install(shared: Arc<SharedInput>) -> Result<Self, InstallError> {
        SHARED
            .set(shared)
            .map_err(|_| InstallError::AlreadyInstalled)?;

        let (ready, started) = std::sync::mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("input-hooks".to_owned())
            .spawn(move || run(&ready))
            .map_err(|_| InstallError::ThreadGone)?;

        match started.recv() {
            Ok(Ok(thread_id)) => Ok(Self {
                thread_id,
                worker: Some(worker),
            }),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(InstallError::ThreadGone),
        }
    }
}

/// Async key state, for modifiers that were already down before the hook saw them.
pub fn os_modifiers() -> Modifiers {
    Modifiers {
        ctrl: is_down(VK_CONTROL),
        shift: is_down(VK_SHIFT),
        alt: is_down(VK_MENU),
        win: is_down(VK_LWIN) || is_down(VK_RWIN),
    }
}

fn is_down(key: VIRTUAL_KEY) -> bool {
    // SAFETY: reads the async key state table, no pointers involved.
    let state = unsafe { GetAsyncKeyState(i32::from(key.0)) };
    (state as u16) & KEY_PRESSED_MASK != 0
}

fn run(ready: &std::sync::mpsc::Sender<Result<u32, InstallError>>) {
    let hooks = match InstalledHooks::new() {
        Ok(hooks) => hooks,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };

    // SAFETY: reads this thread's own id, no pointers involved.
    let thread_id = unsafe { GetCurrentThreadId() };
    if ready.send(Ok(thread_id)).is_err() {
        return;
    }

    pump_messages();
    drop(hooks);
}

/// Owns both hook handles so they come down with the thread.
#[derive(Debug)]
struct InstalledHooks {
    keyboard: HHOOK,
    mouse: HHOOK,
}

impl InstalledHooks {
    fn new() -> Result<Self, InstallError> {
        // SAFETY: both callbacks are static, and thread 0 means a global hook.
        let keyboard = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0) }
            .map_err(refused)?;

        // SAFETY: as above; the keyboard hook is released if this one is refused.
        let mouse = match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), None, 0) } {
            Ok(mouse) => mouse,
            Err(error) => {
                // SAFETY: the handle came from SetWindowsHookExW on this thread.
                let _ = unsafe { UnhookWindowsHookEx(keyboard) };
                return Err(refused(error));
            }
        };

        Ok(Self { keyboard, mouse })
    }
}

fn refused(error: windows::core::Error) -> InstallError {
    InstallError::Refused(format!(
        "SetWindowsHookExW was refused: {error}; a low-level hook needs the process to be running elevated, so start it through the scheduled task or an admin shell"
    ))
}

impl Drop for InstalledHooks {
    fn drop(&mut self) {
        // SAFETY: both handles came from SetWindowsHookExW on this thread.
        unsafe {
            let _ = UnhookWindowsHookEx(self.keyboard);
            let _ = UnhookWindowsHookEx(self.mouse);
        }
    }
}

fn pump_messages() {
    let mut message = MSG::default();

    // SAFETY: the pointer targets a live local; the loop ends on WM_QUIT or an error.
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
        // SAFETY: the message was just filled in by GetMessageW.
        unsafe { DispatchMessageW(&message) };
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: Windows is the caller, so it upholds the HC_ACTION lparam contract.
    if unsafe { keyboard_swallows(code, wparam, lparam) } {
        return SWALLOW;
    }

    // SAFETY: forwards the values Windows handed us unchanged; None lets Windows
    // find the next hook in the chain itself.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Reports whether the key event should be swallowed.
///
/// # Safety
/// For `HC_ACTION`, `lparam` must point at a `KBDLLHOOKSTRUCT`.
unsafe fn keyboard_swallows(code: i32, wparam: WPARAM, lparam: LPARAM) -> bool {
    let Some(shared) = hook_state(code) else {
        return false;
    };

    // SAFETY: upheld by this function's contract.
    let info = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };

    // only our own SendInput is skipped, since remappers like Logi Options+ inject too
    if info.dwExtraInfo == OWN_INPUT_TAG {
        return false;
    }

    let pressed = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
    input::on_key(shared, KeyCode(info.vkCode as u16), pressed)
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: Windows is the caller, so it upholds the HC_ACTION lparam contract.
    if unsafe { mouse_swallows(code, wparam, lparam) } {
        return SWALLOW;
    }

    // SAFETY: forwards the values Windows handed us unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Reports whether the mouse event should be swallowed.
///
/// # Safety
/// For `HC_ACTION`, `lparam` must point at an `MSLLHOOKSTRUCT`.
unsafe fn mouse_swallows(code: i32, wparam: WPARAM, lparam: LPARAM) -> bool {
    let Some(shared) = hook_state(code) else {
        return false;
    };

    // SAFETY: upheld by this function's contract.
    let info = unsafe { *(lparam.0 as *const MSLLHOOKSTRUCT) };

    // our own SendInput must not loop back through the hook
    if info.flags & LLMHF_INJECTED != 0 {
        return false;
    }

    let message = wparam.0 as u32;
    match message {
        WM_MOUSEMOVE => {
            input::on_move(shared, info.pt.x, info.pt.y);
            false
        }
        WM_MOUSEWHEEL => input::on_scroll(shared, high_word(info.mouseData) as i16),
        WM_MOUSEHWHEEL => shared.is_wheel_open(),
        _ => match decode_button(message, info) {
            Some((button, pressed)) => input::on_button(shared, button, pressed),
            None => false,
        },
    }
}

fn decode_button(message: u32, info: MSLLHOOKSTRUCT) -> Option<(MouseButton, bool)> {
    match message {
        WM_LBUTTONDOWN => Some((MouseButton::Left, true)),
        WM_LBUTTONUP => Some((MouseButton::Left, false)),
        WM_RBUTTONDOWN => Some((MouseButton::Right, true)),
        WM_RBUTTONUP => Some((MouseButton::Right, false)),
        WM_MBUTTONDOWN => Some((MouseButton::Middle, true)),
        WM_MBUTTONUP => Some((MouseButton::Middle, false)),
        WM_XBUTTONDOWN => Some((side_button(info), true)),
        WM_XBUTTONUP => Some((side_button(info), false)),
        _ => None,
    }
}

/// Which side button it was lives in the high word of `mouseData`.
fn side_button(info: MSLLHOOKSTRUCT) -> MouseButton {
    match high_word(info.mouseData) == XBUTTON1 {
        true => MouseButton::X1,
        false => MouseButton::X2,
    }
}

/// Both the side button and the scroll amount ride in the high word of `mouseData`.
fn high_word(mouse_data: u32) -> u16 {
    const HIGH_WORD_SHIFT: u32 = u32::BITS / 2;

    (mouse_data >> HIGH_WORD_SHIFT) as u16
}

fn hook_state(code: i32) -> Option<&'static SharedInput> {
    if code != HC_ACTION as i32 {
        return None;
    }
    SHARED.get().map(Arc::as_ref)
}
