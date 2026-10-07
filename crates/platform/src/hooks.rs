//! Low-level keyboard and mouse hooks, on a thread of their own.
//!
//! The hooks do not share the main thread because Windows silently unhooks a callback
//! that overruns `LowLevelHooksTimeout`, and the main thread also renders.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard};
use std::thread::JoinHandle;
use std::time::Instant;

use kile_wheel_core::binding::{
    Action, Binding, BindingTable, KeyCode, Modifiers, MouseButton, Trigger,
};
use thiserror::Error;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PostMessageW, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_XBUTTONDOWN,
    WM_XBUTTONUP, XBUTTON1,
};

/// Posted to the overlay to say the input queue has something in it.
pub const WM_INPUT_READY: u32 = WM_APP + 1;

/// Returned from a hook callback to stop the event reaching the focused app.
const SWALLOW: LRESULT = LRESULT(1);

/// `GetAsyncKeyState` sets the high bit while a key is physically down.
const KEY_PRESSED_MASK: u16 = 0x8000;

/// The hook callbacks have no user-data parameter, so the state they read is global.
static SHARED: OnceLock<Arc<SharedInput>> = OnceLock::new();

/// What the hook decided happened, in wheel terms rather than win32 terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    Hotkey {
        action: Action,
        /// When the hook saw it, so the open latency can be measured end to end.
        seen_at: Instant,
    },
    Key {
        code: KeyCode,
        modifiers: Modifiers,
        pressed: bool,
    },
    MouseMoved {
        x: i32,
        y: i32,
    },
    MouseButton {
        button: MouseButton,
        pressed: bool,
    },
}

/// State the hook callbacks read and the main thread writes.
///
/// Every read on the hook path is one atomic load or one uncontended lock, because a
/// slow callback gets the hook torn down.
#[derive(Debug, Default)]
pub struct SharedInput {
    wheel_open: AtomicBool,
    hotkeys_suppressed: AtomicBool,
    overlay: AtomicIsize,
    wake_pending: AtomicBool,
    bindings: RwLock<BindingTable>,
    queue: Mutex<VecDeque<InputEvent>>,
}

impl SharedInput {
    pub fn set_wheel_open(&self, open: bool) {
        self.wheel_open.store(open, Ordering::Release);
    }

    pub fn is_wheel_open(&self) -> bool {
        self.wheel_open.load(Ordering::Acquire)
    }

    /// Set by the main thread when the foreground window goes fullscreen, so the hook
    /// never has to run that check itself.
    pub fn set_hotkeys_suppressed(&self, suppressed: bool) {
        self.hotkeys_suppressed.store(suppressed, Ordering::Release);
    }

    /// Where the wake message goes. Set once the overlay window exists.
    pub fn set_overlay(&self, overlay: HWND) {
        self.overlay.store(overlay.0 as isize, Ordering::Release);
    }

    pub fn replace_bindings(&self, bindings: BindingTable) {
        *self
            .bindings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = bindings;
    }

    /// Takes everything queued since the last call.
    ///
    /// The wake flag clears first, so an event arriving mid-drain posts a fresh wake
    /// instead of being left to sit.
    pub fn drain(&self) -> VecDeque<InputEvent> {
        self.wake_pending.store(false, Ordering::Release);
        std::mem::take(&mut *self.queue())
    }

    fn emit(&self, event: InputEvent) {
        self.queue().push_back(event);

        if self.wake_pending.swap(true, Ordering::AcqRel) {
            return;
        }
        self.post_wake();
    }

    /// Suppression only applies while the wheel is closed, so a fullscreen app cannot
    /// trap a wheel that is already open.
    fn matched_action(&self, binding: Binding, wheel_open: bool) -> Option<Action> {
        if !wheel_open && self.hotkeys_suppressed.load(Ordering::Acquire) {
            return None;
        }
        self.bindings().action_for(binding, wheel_open)
    }

    fn post_wake(&self) {
        let overlay = self.overlay.load(Ordering::Acquire);
        if overlay == 0 {
            return;
        }

        let target = HWND(overlay as *mut c_void);

        // SAFETY: posting to a window that has gone away fails rather than misbehaves.
        let _ = unsafe { PostMessageW(Some(target), WM_INPUT_READY, WPARAM(0), LPARAM(0)) };
    }

    /// Poisoning only means the main thread panicked; the data itself stays sound, and
    /// a hook callback must not panic.
    fn queue(&self) -> MutexGuard<'_, VecDeque<InputEvent>> {
        self.queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn bindings(&self) -> RwLockReadGuard<'_, BindingTable> {
        self.bindings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("input hooks are already installed; install() runs once per process")]
    AlreadyInstalled,

    #[error(
        "SetWindowsHookExW was refused: {0}; a low-level hook needs the process to be running elevated, so start it through the scheduled task or an admin shell"
    )]
    Refused(#[source] windows::core::Error),

    #[error("the hook thread exited before reporting back, so no hooks are active")]
    ThreadGone,
}

/// Stops the hook thread and removes the hooks on drop.
#[derive(Debug)]
pub struct HookThread {
    thread_id: u32,
    worker: Option<JoinHandle<()>>,
}

impl Drop for HookThread {
    fn drop(&mut self) {
        // SAFETY: posting to a thread that already exited fails rather than misbehaves.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Installs both hooks on a dedicated thread. Callable once per process.
pub fn install(shared: Arc<SharedInput>) -> Result<HookThread, InstallError> {
    SHARED
        .set(shared)
        .map_err(|_| InstallError::AlreadyInstalled)?;

    let (ready, started) = std::sync::mpsc::channel();
    let worker = std::thread::Builder::new()
        .name("input-hooks".to_owned())
        .spawn(move || run(&ready))
        .map_err(|_| InstallError::ThreadGone)?;

    match started.recv() {
        Ok(Ok(thread_id)) => Ok(HookThread {
            thread_id,
            worker: Some(worker),
        }),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(InstallError::ThreadGone),
    }
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
            .map_err(InstallError::Refused)?;

        // SAFETY: as above; the keyboard hook is released if this one is refused.
        let mouse = match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), None, 0) } {
            Ok(mouse) => mouse,
            Err(error) => {
                // SAFETY: the handle came from SetWindowsHookExW on this thread.
                let _ = unsafe { UnhookWindowsHookEx(keyboard) };
                return Err(InstallError::Refused(error));
            }
        };

        Ok(Self { keyboard, mouse })
    }
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

    // our own SendInput must not loop back through the hook
    if info.flags.0 & LLKHF_INJECTED.0 != 0 {
        return false;
    }

    let key = KeyCode(info.vkCode as u16);
    let trigger = Trigger::Key(key);
    let modifiers = current_modifiers();

    if !matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN) {
        return on_release(
            shared,
            trigger,
            InputEvent::Key {
                code: key,
                modifiers,
                pressed: false,
            },
        );
    }

    on_press(
        shared,
        trigger,
        modifiers,
        InputEvent::Key {
            code: key,
            modifiers,
            pressed: true,
        },
    )
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
/// Movement always passes through, because the cursor has to keep aiming the wheel.
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
    if message == WM_MOUSEMOVE {
        if shared.is_wheel_open() {
            shared.emit(InputEvent::MouseMoved {
                x: info.pt.x,
                y: info.pt.y,
            });
        }
        return false;
    }

    // scrolling into the app underneath an open wheel is never what was meant
    if matches!(message, WM_MOUSEWHEEL | WM_MOUSEHWHEEL) {
        return shared.is_wheel_open();
    }

    let Some((button, pressed)) = decode_button(message, info) else {
        return false;
    };
    let trigger = Trigger::Mouse(button);
    let event = InputEvent::MouseButton { button, pressed };

    if !pressed {
        return on_release(shared, trigger, event);
    }
    on_press(shared, trigger, current_modifiers(), event)
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
    const HIGH_WORD_SHIFT: u32 = u32::BITS / 2;

    match (info.mouseData >> HIGH_WORD_SHIFT) as u16 == XBUTTON1 {
        true => MouseButton::X1,
        false => MouseButton::X2,
    }
}

/// Shared press path for keys and buttons. Reports whether to swallow.
///
/// The binding is built here rather than by the callers, so the key actually pressed is
/// what gets remembered even when it only completed a modifier-only combo.
fn on_press(
    shared: &SharedInput,
    trigger: Trigger,
    modifiers: Modifiers,
    event: InputEvent,
) -> bool {
    let wheel_open = shared.is_wheel_open();
    let binding = match trigger {
        Trigger::Key(key) => Binding::for_key_press(key, modifiers),
        Trigger::Mouse(button) => Binding::for_button_press(button, modifiers),
    };

    if let Some(action) = shared.matched_action(binding, wheel_open) {
        shared.emit(InputEvent::Hotkey {
            action,
            seen_at: Instant::now(),
        });
        remember(trigger);
        return true;
    }
    if !wheel_open {
        return false;
    }

    // everything is swallowed while the wheel is open, so nothing leaks into the app
    shared.emit(event);
    remember(trigger);
    true
}

/// Shared release path. Reports whether to swallow.
///
/// Only a release whose press was swallowed is swallowed, so an app never sees a
/// release it had no press for.
fn on_release(shared: &SharedInput, trigger: Trigger, event: InputEvent) -> bool {
    if !forget(trigger) {
        return false;
    }
    if shared.is_wheel_open() {
        shared.emit(event);
    }
    true
}

fn hook_state(code: i32) -> Option<&'static SharedInput> {
    if code != HC_ACTION as i32 {
        return None;
    }
    SHARED.get().map(Arc::as_ref)
}

fn current_modifiers() -> Modifiers {
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

std::thread_local! {
    /// Triggers whose press this hook swallowed, so the matching release goes too.
    /// Thread-local because only the hook thread ever touches it.
    static SWALLOWED: std::cell::RefCell<Vec<Trigger>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn remember(trigger: Trigger) {
    SWALLOWED.with_borrow_mut(|held| {
        if held.contains(&trigger) {
            return;
        }
        held.push(trigger);
    });
}

/// Reports whether the press was swallowed, and forgets it either way.
fn forget(trigger: Trigger) -> bool {
    SWALLOWED.with_borrow_mut(|held| {
        let Some(at) = held.iter().position(|candidate| *candidate == trigger) else {
            return false;
        };
        held.swap_remove(at);
        true
    })
}
