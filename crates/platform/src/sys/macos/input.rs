//! A session event tap, on a thread with its own run loop.
//!
//! Returning null from the tap swallows an event. macOS turns a slow tap off, so the
//! callback only translates and decides, and turns the tap back on if that ever happens.

use std::ffi::c_void;
use std::ptr::{NonNull, null_mut};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Arc, OnceLock};

use kile_wheel_core::binding::{Modifiers, MouseButton, keys};
use objc2_core_foundation::{CFMachPort, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventSource, CGEventSourceStateID,
    CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
};

use super::{ACCESSIBILITY_HINT, ax, keymap};
use crate::input::{self, InstallError, SharedInput};

/// Marks events we post ourselves, so the tap passes them through.
pub const OWN_INPUT_TAG: i64 = 0x4B57_3930;

/// Win32 reports one scroll notch as 120, which is what the wheel expects.
const NOTCH: i16 = 120;

/// macOS numbers the buttons past left and right from two.
const MIDDLE_BUTTON: i64 = 2;
const BACK_BUTTON: i64 = 3;
const FORWARD_BUTTON: i64 = 4;

const LISTENED: [CGEventType; 14] = [
    CGEventType::KeyDown,
    CGEventType::KeyUp,
    CGEventType::FlagsChanged,
    CGEventType::LeftMouseDown,
    CGEventType::LeftMouseUp,
    CGEventType::RightMouseDown,
    CGEventType::RightMouseUp,
    CGEventType::OtherMouseDown,
    CGEventType::OtherMouseUp,
    CGEventType::MouseMoved,
    CGEventType::LeftMouseDragged,
    CGEventType::RightMouseDragged,
    CGEventType::OtherMouseDragged,
    CGEventType::ScrollWheel,
];

static SHARED: OnceLock<Arc<SharedInput>> = OnceLock::new();

/// The tap, so the callback can turn it back on after macOS disables it.
static TAP: AtomicPtr<CFMachPort> = AtomicPtr::new(null_mut());

/// The tap thread runs for the process lifetime.
#[derive(Debug)]
pub struct Capture {
    _private: (),
}

impl Capture {
    pub fn install(shared: Arc<SharedInput>) -> Result<Self, InstallError> {
        SHARED
            .set(shared)
            .map_err(|_| InstallError::AlreadyInstalled)?;
        if !ax::trusted() {
            return Err(InstallError::Refused(format!(
                "macOS has not allowed the wheel to see keys; {ACCESSIBILITY_HINT}"
            )));
        }

        let (ready, started) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("input-tap".to_owned())
            .spawn(move || run(&ready))
            .map_err(|_| InstallError::ThreadGone)?;

        match started.recv() {
            Ok(Ok(())) => Ok(Self { _private: () }),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(InstallError::ThreadGone),
        }
    }
}

/// The session's own idea of the held modifiers.
pub fn os_modifiers() -> Modifiers {
    modifiers_of(CGEventSource::flags_state(CGEventSourceStateID::CombinedSessionState))
}

fn modifiers_of(flags: CGEventFlags) -> Modifiers {
    Modifiers {
        ctrl: flags.contains(CGEventFlags::MaskControl),
        shift: flags.contains(CGEventFlags::MaskShift),
        alt: flags.contains(CGEventFlags::MaskAlternate),
        win: flags.contains(CGEventFlags::MaskCommand),
    }
}

fn run(ready: &std::sync::mpsc::Sender<Result<(), InstallError>>) {
    let mask: CGEventMask = LISTENED
        .iter()
        .fold(0, |mask, kind| mask | (1 << kind.0));

    // SAFETY: the callback is a static function, and it reads no user data.
    let tap = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            mask,
            Some(on_event),
            null_mut(),
        )
    };
    let Some(tap) = tap else {
        let _ = ready.send(Err(InstallError::Refused(format!(
            "macOS refused the event tap; {ACCESSIBILITY_HINT}, and allow Input Monitoring too if it asks"
        ))));
        return;
    };
    TAP.store(objc2_core_foundation::CFRetained::as_ptr(&tap).as_ptr(), Ordering::Release);

    let Some(source) = CFMachPort::new_run_loop_source(None, Some(&tap), 0) else {
        let _ = ready.send(Err(InstallError::Refused(
            "could not attach the event tap to a run loop".to_owned(),
        )));
        return;
    };
    let Some(run_loop) = CFRunLoop::current() else {
        let _ = ready.send(Err(InstallError::ThreadGone));
        return;
    };
    // SAFETY: the mode constant is a static CoreFoundation owns for the process lifetime.
    run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
    CGEvent::tap_enable(&tap, true);

    if ready.send(Ok(())).is_err() {
        return;
    }
    CFRunLoop::run();
}

unsafe extern "C-unwind" fn on_event(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    _user_info: *mut c_void,
) -> *mut CGEvent {
    if matches!(
        kind,
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
    ) {
        reenable();
        return event.as_ptr();
    }

    // SAFETY: macOS hands the callback a live event for the duration of the call.
    let event_ref = unsafe { event.as_ref() };
    match SHARED.get() {
        Some(shared) if swallows(shared, kind, event_ref) => null_mut(),
        _ => event.as_ptr(),
    }
}

fn reenable() {
    let tap = TAP.load(Ordering::Acquire);
    // SAFETY: the tap is stored once and never released while the thread runs.
    if let Some(tap) = unsafe { tap.as_ref() } {
        CGEvent::tap_enable(tap, true);
    }
}

fn swallows(shared: &SharedInput, kind: CGEventType, event: &CGEvent) -> bool {
    let field = |field: CGEventField| CGEvent::integer_value_field(Some(event), field);
    if field(CGEventField::EventSourceUserData) == OWN_INPUT_TAG {
        return false;
    }

    match kind {
        CGEventType::KeyDown | CGEventType::KeyUp => {
            let Some(key) = keymap::to_vk(field(CGEventField::KeyboardEventKeycode) as u16) else {
                return false;
            };
            input::on_key(shared, key, kind == CGEventType::KeyDown)
        }
        CGEventType::FlagsChanged => on_flags(shared, event, field(CGEventField::KeyboardEventKeycode) as u16),
        CGEventType::LeftMouseDown => input::on_button(shared, MouseButton::Left, true),
        CGEventType::LeftMouseUp => input::on_button(shared, MouseButton::Left, false),
        CGEventType::RightMouseDown => input::on_button(shared, MouseButton::Right, true),
        CGEventType::RightMouseUp => input::on_button(shared, MouseButton::Right, false),
        CGEventType::OtherMouseDown | CGEventType::OtherMouseUp => {
            let Some(button) = other_button(field(CGEventField::MouseEventButtonNumber)) else {
                return false;
            };
            input::on_button(shared, button, kind == CGEventType::OtherMouseDown)
        }
        CGEventType::ScrollWheel => {
            let notches = field(CGEventField::ScrollWheelEventDeltaAxis1).signum() as i16;
            input::on_scroll(shared, notches * NOTCH)
        }
        _ => {
            let at = CGEvent::location(Some(event));
            input::on_move(shared, at.x as i32, at.y as i32);
            false
        }
    }
}

/// A modifier, or Caps Lock, changed. Which way is read from the flags it left behind.
///
/// Caps Lock only reports its toggles, so each one is a press and a release; macOS has
/// already flipped the caps state by then, so binding it to the wheel also toggles caps.
fn on_flags(shared: &SharedInput, event: &CGEvent, code: u16) -> bool {
    let Some(key) = keymap::to_vk(code) else {
        return false;
    };
    if key == keys::CAPS_LOCK {
        let swallowed = input::on_key(shared, key, true);
        input::on_key(shared, key, false);
        return swallowed;
    }

    let flags = CGEvent::flags(Some(event));
    let held = modifiers_of(flags);
    let pressed = match key {
        keys::LEFT_SHIFT | keys::RIGHT_SHIFT => held.shift,
        keys::LEFT_CTRL | keys::RIGHT_CTRL => held.ctrl,
        keys::LEFT_ALT | keys::RIGHT_ALT => held.alt,
        keys::LEFT_WIN | keys::RIGHT_WIN => held.win,
        _ => return false,
    };
    input::on_key(shared, key, pressed)
}

fn other_button(number: i64) -> Option<MouseButton> {
    match number {
        MIDDLE_BUTTON => Some(MouseButton::Middle),
        BACK_BUTTON => Some(MouseButton::X1),
        FORWARD_BUTTON => Some(MouseButton::X2),
        _ => None,
    }
}
