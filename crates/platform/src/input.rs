//! Global keyboard and mouse input, and the decision of what to swallow.
//!
//! Each OS has its own way to see every key and button before the focused app does: a
//! low-level hook on Windows, an event tap on macOS, a grabbed evdev device on Linux. The
//! backends only translate their events and ask this module what to do with each one, so
//! hotkey matching and swallowing behave the same everywhere.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard};
use std::time::Instant;

use kile_wheel_core::binding::{
    Action, Binding, BindingTable, KeyCode, Modifiers, MouseButton, Trigger, keys,
};
use thiserror::Error;

use crate::event_loop::Waker;
use crate::sys;

/// What the backend decided happened, in wheel terms rather than OS terms.
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
    /// The cursor, in screen units.
    MouseMoved {
        x: i32,
        y: i32,
    },
    MouseButton {
        button: MouseButton,
        pressed: bool,
    },
    /// The vertical scroll wheel. Positive is away from you.
    Scrolled {
        delta: i16,
    },
}

/// State the backends read and the main thread writes.
///
/// Every read on the hook path is one atomic load or one uncontended lock, because a
/// slow callback gets a Windows hook torn down and stalls input everywhere else.
#[derive(Debug, Default)]
pub struct SharedInput {
    wheel_open: AtomicBool,
    hotkeys_suppressed: AtomicBool,
    wake_pending: AtomicBool,
    waker: OnceLock<Waker>,
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

    /// Where queued input wakes. Set once, before the backend is installed.
    pub fn set_waker(&self, waker: Waker) {
        let _ = self.waker.set(waker);
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

    pub(crate) fn emit(&self, event: InputEvent) {
        self.queue().push_back(event);

        if self.wake_pending.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(waker) = self.waker.get() {
            waker.wake();
        }
    }

    /// Suppression only applies while the wheel is closed, so a fullscreen app cannot
    /// trap a wheel that is already open.
    fn matched_action(&self, binding: Binding, wheel_open: bool) -> Option<Action> {
        if !wheel_open && self.hotkeys_suppressed.load(Ordering::Acquire) {
            return None;
        }
        self.bindings().action_for(binding, wheel_open)
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
    #[error("input capture is already running; install() runs once per process")]
    AlreadyInstalled,

    /// The OS said no. The text says why and what to change, which differs per OS.
    #[error("{0}")]
    Refused(String),

    #[error("the input thread exited before reporting back, so no hotkey is active")]
    ThreadGone,
}

/// Stops capturing input on drop.
#[derive(Debug)]
pub struct InputCapture {
    _sys: sys::input::Capture,
}

/// Starts capturing on a dedicated thread. Callable once per process.
pub fn install(shared: Arc<SharedInput>) -> Result<InputCapture, InstallError> {
    Ok(InputCapture {
        _sys: sys::input::Capture::install(shared)?,
    })
}

/// A key went down or up. Reports whether the focused app must not see it.
pub(crate) fn on_key(shared: &SharedInput, key: KeyCode, pressed: bool) -> bool {
    // tracked before the modifiers are read, so a modifier press is already counted by the
    // time the key it modifies arrives
    note_modifier(key, pressed);
    let modifiers = current_modifiers();
    let event = InputEvent::Key {
        code: key,
        modifiers,
        pressed,
    };

    match pressed {
        true => on_press(shared, Trigger::Key(key), modifiers, event),
        false => on_release(shared, Trigger::Key(key), event),
    }
}

/// A mouse button went down or up. Reports whether the focused app must not see it.
pub(crate) fn on_button(shared: &SharedInput, button: MouseButton, pressed: bool) -> bool {
    let event = InputEvent::MouseButton { button, pressed };

    match pressed {
        true => on_press(shared, Trigger::Mouse(button), current_modifiers(), event),
        false => on_release(shared, Trigger::Mouse(button), event),
    }
}

/// Scrolling into the app underneath an open wheel is never what was meant, so it is
/// swallowed while the wheel is up. `delta` is positive away from you.
pub(crate) fn on_scroll(shared: &SharedInput, delta: i16) -> bool {
    if !shared.is_wheel_open() {
        return false;
    }
    shared.emit(InputEvent::Scrolled { delta });
    true
}

/// Movement always passes through, because the cursor has to keep aiming the wheel.
pub(crate) fn on_move(shared: &SharedInput, x: i32, y: i32) {
    if shared.is_wheel_open() {
        shared.emit(InputEvent::MouseMoved { x, y });
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

fn current_modifiers() -> Modifiers {
    let tracked = tracked_modifiers();
    let reported = sys::input::os_modifiers();

    // the two sources are OR'ed because neither is complete on its own: a key the backend
    // swallowed never reaches the OS's own key state, and a key already down before the
    // backend started was never seen here
    Modifiers {
        ctrl: tracked.ctrl || reported.ctrl,
        shift: tracked.shift || reported.shift,
        alt: tracked.alt || reported.alt,
        win: tracked.win || reported.win,
    }
}

/// Only what the backend has seen, with no reference to the real keyboard. Split out from
/// [`current_modifiers`] so it can be tested without a physically held key deciding the
/// answer.
fn tracked_modifiers() -> Modifiers {
    TRACKED_MODIFIERS.with(|held| held.get())
}

std::thread_local! {
    /// Modifiers seen pressed and not yet released.
    ///
    /// Needed because swallowing a key stops the OS from recording it: while the wheel is
    /// open it eats everything, so the OS would report Shift and Ctrl as up and the form
    /// would type "v" instead of pasting. Thread-local because each backend delivers
    /// keys on one thread of its own.
    static TRACKED_MODIFIERS: std::cell::Cell<Modifiers> = const {
        std::cell::Cell::new(Modifiers {
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
        })
    };
}

/// Records a modifier going down or coming up. Other keys are ignored.
fn note_modifier(key: KeyCode, pressed: bool) {
    let Some(which) = modifier_of(key) else {
        return;
    };

    TRACKED_MODIFIERS.with(|held| {
        let mut modifiers = held.get();
        *which.field(&mut modifiers) = pressed;
        held.set(modifiers);
    });
}

/// Which modifier a key is, counting the left and right variants as the same one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Win,
}

impl Modifier {
    fn field(self, modifiers: &mut Modifiers) -> &mut bool {
        match self {
            Self::Ctrl => &mut modifiers.ctrl,
            Self::Shift => &mut modifiers.shift,
            Self::Alt => &mut modifiers.alt,
            Self::Win => &mut modifiers.win,
        }
    }
}

fn modifier_of(key: KeyCode) -> Option<Modifier> {
    let found = match key {
        keys::CTRL | keys::LEFT_CTRL | keys::RIGHT_CTRL => Modifier::Ctrl,
        keys::SHIFT | keys::LEFT_SHIFT | keys::RIGHT_SHIFT => Modifier::Shift,
        keys::ALT | keys::LEFT_ALT | keys::RIGHT_ALT => Modifier::Alt,
        keys::LEFT_WIN | keys::RIGHT_WIN => Modifier::Win,
        _ => return None,
    };
    Some(found)
}

std::thread_local! {
    /// Triggers whose press was swallowed, so the matching release goes too.
    /// Thread-local because only the input thread ever touches it.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs on a fresh thread each time, because the tracker is thread-local and these
    /// cases must not see each other's state.
    fn on_input_thread(body: impl FnOnce() + Send + 'static) {
        std::thread::spawn(body).join().expect("the case panicked");
    }

    fn caps_lock_toggles() -> SharedInput {
        let shared = SharedInput::default();
        shared.replace_bindings(BindingTable::new(vec![kile_wheel_core::binding::Hotkey {
            binding: Binding::key(keys::CAPS_LOCK),
            action: Action::Toggle,
        }]));
        shared
    }

    #[test]
    fn the_left_and_right_variants_are_the_same_modifier() {
        assert_eq!(modifier_of(keys::LEFT_SHIFT), Some(Modifier::Shift));
        assert_eq!(modifier_of(keys::RIGHT_SHIFT), Some(Modifier::Shift));
        assert_eq!(modifier_of(keys::SHIFT), Some(Modifier::Shift));
    }

    #[test]
    fn every_modifier_key_is_recognised() {
        for key in [keys::LEFT_CTRL, keys::RIGHT_CTRL, keys::CTRL] {
            assert_eq!(modifier_of(key), Some(Modifier::Ctrl));
        }
        for key in [keys::LEFT_ALT, keys::RIGHT_ALT, keys::ALT] {
            assert_eq!(modifier_of(key), Some(Modifier::Alt));
        }
        for key in [keys::LEFT_WIN, keys::RIGHT_WIN] {
            assert_eq!(modifier_of(key), Some(Modifier::Win));
        }
    }

    #[test]
    fn an_ordinary_key_is_not_a_modifier() {
        assert_eq!(modifier_of(keys::W), None);
    }

    /// The reason this tracker exists: a swallowed Shift never reaches the OS key state,
    /// so the form would see no Shift and type a lower-case letter.
    #[test]
    fn a_tracked_shift_is_remembered_even_though_the_os_never_saw_it() {
        on_input_thread(|| {
            assert!(!tracked_modifiers().shift, "nothing tracked yet");

            note_modifier(keys::LEFT_SHIFT, true);
            assert!(tracked_modifiers().shift, "the press should be tracked");

            note_modifier(keys::LEFT_SHIFT, false);
            assert!(!tracked_modifiers().shift, "the release should clear it");
        });
    }

    #[test]
    fn an_ordinary_key_leaves_the_tracked_modifiers_alone() {
        on_input_thread(|| {
            note_modifier(keys::LEFT_SHIFT, true);
            note_modifier(keys::W, true);
            note_modifier(keys::W, false);

            assert!(tracked_modifiers().shift, "W must not clear shift");
        });
    }

    /// Whatever the real keyboard is doing, a tracked press must still come through.
    #[test]
    fn the_reported_modifiers_never_drop_a_tracked_one() {
        on_input_thread(|| {
            note_modifier(keys::LEFT_SHIFT, true);

            assert!(current_modifiers().shift);
        });
    }

    #[test]
    fn the_hotkey_is_swallowed_and_queued() {
        on_input_thread(|| {
            let shared = caps_lock_toggles();

            assert!(on_key(&shared, keys::CAPS_LOCK, true));
            assert!(matches!(
                shared.drain().pop_front(),
                Some(InputEvent::Hotkey {
                    action: Action::Toggle,
                    ..
                })
            ));
            // the release of a swallowed press goes too, so the app never sees half a key
            assert!(on_key(&shared, keys::CAPS_LOCK, false));
        });
    }

    #[test]
    fn other_keys_pass_through_while_the_wheel_is_closed() {
        on_input_thread(|| {
            let shared = caps_lock_toggles();

            assert!(!on_key(&shared, keys::W, true));
            assert!(!on_key(&shared, keys::W, false));
            assert!(shared.drain().is_empty());
        });
    }

    #[test]
    fn every_key_is_swallowed_while_the_wheel_is_open() {
        on_input_thread(|| {
            let shared = caps_lock_toggles();
            shared.set_wheel_open(true);

            assert!(on_key(&shared, keys::W, true));
            assert!(on_button(&shared, MouseButton::Left, true));
            assert!(on_scroll(&shared, 120));
            assert_eq!(shared.drain().len(), 3);
        });
    }

    #[test]
    fn a_fullscreen_app_suppresses_the_hotkey_until_the_wheel_is_open() {
        on_input_thread(|| {
            let shared = caps_lock_toggles();
            shared.set_hotkeys_suppressed(true);

            assert!(!on_key(&shared, keys::CAPS_LOCK, true));
            assert!(!on_key(&shared, keys::CAPS_LOCK, false));
        });
    }
}
