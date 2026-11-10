//! Keystrokes posted into the session, tagged so our own tap lets them through.

use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{CGEvent, CGEventField, CGEventFlags, CGEventTapLocation};

use super::input::OWN_INPUT_TAG;
use crate::send::SendError;

/// kVK_ANSI_V, which is where Cmd+V lives whatever the layout.
const V_KEY: u16 = 9;
const RETURN_KEY: u16 = 36;

/// The virtual key a unicode event rides on; the text replaces whatever it would type.
const ANY_KEY: u16 = 0;

pub fn paste() -> Result<(), SendError> {
    post_key(V_KEY, true, CGEventFlags::MaskCommand)?;
    post_key(V_KEY, false, CGEventFlags::MaskCommand)
}

pub fn text(body: &str) -> Result<(), SendError> {
    for character in body.chars().filter(|character| *character != '\r') {
        if character == '\n' {
            post_key(RETURN_KEY, true, CGEventFlags::empty())?;
            post_key(RETURN_KEY, false, CGEventFlags::empty())?;
            continue;
        }
        let mut units = [0u16; 2];
        let units = character.encode_utf16(&mut units);
        post_text(units, true)?;
        post_text(units, false)?;
    }
    Ok(())
}

fn post_key(code: u16, down: bool, flags: CGEventFlags) -> Result<(), SendError> {
    let event = keyboard_event(code, down)?;
    CGEvent::set_flags(Some(&event), flags);
    post(&event);
    Ok(())
}

fn post_text(units: &[u16], down: bool) -> Result<(), SendError> {
    let event = keyboard_event(ANY_KEY, down)?;
    // SAFETY: the pointer and length describe the same live slice.
    unsafe { CGEvent::keyboard_set_unicode_string(Some(&event), units.len() as _, units.as_ptr()) };
    post(&event);
    Ok(())
}

fn keyboard_event(code: u16, down: bool) -> Result<CFRetained<CGEvent>, SendError> {
    CGEvent::new_keyboard_event(None, code, down)
        .ok_or_else(|| SendError("macOS would not create a keyboard event".to_owned()))
}

fn post(event: &CGEvent) {
    CGEvent::set_integer_value_field(Some(event), CGEventField::EventSourceUserData, OWN_INPUT_TAG);
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(event));
}

#[cfg(test)]
mod tests {
    use super::super::keymap;
    use super::*;
    use kile_wheel_core::binding::keys;

    #[test]
    fn v_is_where_the_keymap_says() {
        assert_eq!(keymap::from_vk(keys::V), Some(V_KEY));
        assert_eq!(keymap::from_vk(keys::ENTER), Some(RETURN_KEY));
    }
}
