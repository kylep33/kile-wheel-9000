//! Synthesising keystrokes for snippet delivery. Everything here is injected, which the
//! low-level hook filters out, so none of it loops back into the wheel.

use thiserror::Error;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_RETURN, VK_V,
};

#[derive(Debug, Error)]
#[error("only {sent} of {wanted} keystrokes were accepted, so the snippet is incomplete")]
pub struct SendError {
    sent: usize,
    wanted: usize,
}

/// A unicode event carries the character in `wScan` and leaves the key code at zero.
const NO_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0);

/// Characters above the basic plane arrive as two surrogates, so one char can need two
/// events, and a press plus a release doubles that again.
const EVENTS_PER_CHAR: usize = 4;

/// Sends Ctrl+V to whatever holds focus.
pub fn paste() -> Result<(), SendError> {
    let events = [
        key_event(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
        key_event(VK_V, KEYBD_EVENT_FLAGS(0)),
        key_event(VK_V, KEYEVENTF_KEYUP),
        key_event(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    dispatch(&events)
}

/// Types `text` character by character. Newlines go as Enter, because a bare line feed is
/// ignored by most controls.
pub fn text(body: &str) -> Result<(), SendError> {
    let mut events = Vec::with_capacity(body.len() * EVENTS_PER_CHAR);

    for character in body.chars() {
        match character {
            '\r' => continue,
            '\n' => push_key(&mut events, VK_RETURN),
            _ => push_char(&mut events, character),
        }
    }

    match events.is_empty() {
        true => Ok(()),
        false => dispatch(&events),
    }
}

fn push_key(events: &mut Vec<INPUT>, key: VIRTUAL_KEY) {
    events.push(key_event(key, KEYBD_EVENT_FLAGS(0)));
    events.push(key_event(key, KEYEVENTF_KEYUP));
}

fn push_char(events: &mut Vec<INPUT>, character: char) {
    let mut buffer = [0u16; 2];

    for unit in character.encode_utf16(&mut buffer) {
        events.push(unicode_event(*unit, KEYEVENTF_UNICODE));
        events.push(unicode_event(*unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
    }
}

fn key_event(key: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    input_of(KEYBDINPUT {
        wVk: key,
        wScan: 0,
        dwFlags: flags,
        time: 0,
        dwExtraInfo: 0,
    })
}

fn unicode_event(unit: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    input_of(KEYBDINPUT {
        wVk: NO_KEY,
        wScan: unit,
        dwFlags: flags,
        time: 0,
        dwExtraInfo: 0,
    })
}

fn input_of(keyboard: KEYBDINPUT) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: keyboard },
    }
}

/// One call for the whole batch, so nothing else can interleave keystrokes into it.
fn dispatch(events: &[INPUT]) -> Result<(), SendError> {
    // SAFETY: the slice is live for the call and the size matches the struct it describes.
    let sent = unsafe { SendInput(events, size_of::<INPUT>() as i32) } as usize;

    if sent != events.len() {
        return Err(SendError {
            sent,
            wanted: events.len(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The event list is built without touching the OS, so the shape is worth pinning.
    fn events_for(body: &str) -> Vec<INPUT> {
        let mut events = Vec::new();

        for character in body.chars() {
            match character {
                '\r' => continue,
                '\n' => push_key(&mut events, VK_RETURN),
                _ => push_char(&mut events, character),
            }
        }
        events
    }

    #[test]
    fn a_plain_character_sends_a_press_and_a_release() {
        let events = events_for("a");

        assert_eq!(events.len(), 2);
        // SAFETY: every event built here is a keyboard event.
        let first = unsafe { events[0].Anonymous.ki };
        assert_eq!(first.wScan, u16::from(b'a'));
        assert_eq!(first.wVk, NO_KEY);
    }

    #[test]
    fn a_newline_becomes_enter_rather_than_a_line_feed() {
        let events = events_for("\n");

        assert_eq!(events.len(), 2);
        // SAFETY: every event built here is a keyboard event.
        let first = unsafe { events[0].Anonymous.ki };
        assert_eq!(first.wVk, VK_RETURN);
    }

    #[test]
    fn a_carriage_return_is_dropped_so_crlf_sends_one_enter() {
        assert_eq!(events_for("\r\n").len(), 2);
    }

    #[test]
    fn a_character_outside_the_basic_plane_sends_both_surrogates() {
        // a musical symbol, which is one char but two utf-16 units
        assert_eq!(events_for("\u{1d11e}").len(), 4);
    }

    #[test]
    fn an_empty_snippet_builds_no_events() {
        assert!(events_for("").is_empty());
    }
}
