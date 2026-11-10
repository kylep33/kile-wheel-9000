//! Keystrokes into the focused window, through the virtual device. The input thread never
//! reads that device, so none of this loops back into the wheel.

use evdev::{EventType, InputEvent};
use kile_wheel_core::binding::{KeyCode, keys};

use super::{keymap, uinput};
use crate::send::SendError;

const PRESSED: i32 = 1;
const RELEASED: i32 = 0;

/// Ctrl+V; terminals want Ctrl+Shift+V, which is the same trade-off Windows makes.
pub fn paste() -> Result<(), SendError> {
    chord(&[keys::LEFT_CTRL], keys::V)
}

/// Plain ASCII only. Evdev sends physical keys, so anything a US keyboard cannot type
/// directly has no keystroke to send; clipboard mode handles everything.
pub fn text(body: &str) -> Result<(), SendError> {
    let strokes = body
        .chars()
        .filter(|character| *character != '\r')
        .map(|character| {
            stroke_for(character).ok_or_else(|| {
                SendError(format!(
                    "'{character}' cannot be typed as keystrokes on Linux; switch the snippet to clipboard mode"
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    for (key, shifted) in strokes {
        let modifiers: &[KeyCode] = match shifted {
            true => &[keys::LEFT_SHIFT],
            false => &[],
        };
        chord(modifiers, key)?;
    }
    Ok(())
}

fn chord(modifiers: &[KeyCode], key: KeyCode) -> Result<(), SendError> {
    for modifier in modifiers {
        send(*modifier, PRESSED)?;
    }
    send(key, PRESSED)?;
    send(key, RELEASED)?;
    for modifier in modifiers.iter().rev() {
        send(*modifier, RELEASED)?;
    }
    Ok(())
}

fn send(key: KeyCode, value: i32) -> Result<(), SendError> {
    let code = keymap::from_vk(key)
        .ok_or_else(|| SendError(format!("virtual key {:#x} has no Linux key code", key.0)))?;
    uinput::emit(&[InputEvent::new(EventType::KEY.0, code, value)]).map_err(SendError)
}

/// The US-layout key and whether Shift is needed.
fn stroke_for(character: char) -> Option<(KeyCode, bool)> {
    const SHIFTED_DIGITS: &str = ")!@#$%^&*(";
    const PUNCTUATION: &[(u16, char, char)] = &[
        (0xBA, ';', ':'),
        (0xBB, '=', '+'),
        (0xBC, ',', '<'),
        (0xBD, '-', '_'),
        (0xBE, '.', '>'),
        (0xBF, '/', '?'),
        (0xC0, '`', '~'),
        (0xDB, '[', '{'),
        (0xDC, '\\', '|'),
        (0xDD, ']', '}'),
        (0xDE, '\'', '"'),
    ];

    match character {
        '\n' => return Some((keys::ENTER, false)),
        '\t' => return Some((keys::TAB, false)),
        ' ' => return Some((keys::SPACE, false)),
        'a'..='z' => return Some((KeyCode(character.to_ascii_uppercase() as u16), false)),
        'A'..='Z' | '0'..='9' => return Some((KeyCode(character as u16), character.is_ascii_uppercase())),
        _ => {}
    }
    if let Some(digit) = SHIFTED_DIGITS.find(character) {
        return Some((KeyCode(u16::from(b'0') + digit as u16), true));
    }
    PUNCTUATION.iter().find_map(|(code, plain, shifted)| {
        (character == *plain)
            .then_some((KeyCode(*code), false))
            .or((character == *shifted).then_some((KeyCode(*code), true)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_and_symbols_have_strokes() {
        assert_eq!(stroke_for('w'), Some((keys::W, false)));
        assert_eq!(stroke_for('W'), Some((keys::W, true)));
        assert_eq!(stroke_for('!'), Some((keys::DIGIT_1, true)));
        assert_eq!(stroke_for(':'), Some((KeyCode(0xBA), true)));
        assert_eq!(stroke_for('\n'), Some((keys::ENTER, false)));
    }

    #[test]
    fn a_character_off_the_keyboard_has_none() {
        assert_eq!(stroke_for('é'), None);
    }

    #[test]
    fn every_stroke_has_a_linux_key() {
        for character in (' '..='~').chain(['\n', '\t']) {
            let (key, _) = stroke_for(character).expect("printable ascii");
            assert!(keymap::from_vk(key).is_some(), "{character:?}");
        }
    }
}
