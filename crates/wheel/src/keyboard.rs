//! Turning a virtual key into the character it types.
//!
//! The hook hands over virtual keys, not characters. Asking Windows to translate would use
//! the keyboard layout of whatever app had focus, which is not necessarily the one the
//! user is typing with, so the shifted forms are spelled out here instead.

use kile_wheel_core::binding::{KeyCode, Modifiers, keys};

/// Digits 0 to 9 with Shift held, on a US layout.
const SHIFTED_DIGITS: [char; 10] = [')', '!', '@', '#', '$', '%', '^', '&', '*', '('];

/// `VK_OEM_*` keys, as (code, unshifted, shifted) on a US layout.
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

/// The character a key types, or `None` when it is not a typing key.
pub fn printable(code: KeyCode, modifiers: Modifiers) -> Option<char> {
    // a combo belongs to the wheel or the field's own handling, not to typing
    if modifiers.ctrl || modifiers.alt || modifiers.win {
        return None;
    }
    if code == keys::SPACE {
        return Some(' ');
    }

    if let Some(character) = alphanumeric(code, modifiers.shift) {
        return Some(character);
    }
    punctuation(code, modifiers.shift)
}

fn alphanumeric(code: KeyCode, shifted: bool) -> Option<char> {
    let raw = char::from(u8::try_from(code.0).ok()?);

    if raw.is_ascii_alphabetic() {
        return Some(match shifted {
            true => raw.to_ascii_uppercase(),
            false => raw.to_ascii_lowercase(),
        });
    }
    if !raw.is_ascii_digit() {
        return None;
    }

    let digit = usize::from(raw as u8 - b'0');
    Some(match shifted {
        true => SHIFTED_DIGITS[digit],
        false => raw,
    })
}

fn punctuation(code: KeyCode, shifted: bool) -> Option<char> {
    PUNCTUATION
        .iter()
        .find(|(candidate, _, _)| *candidate == code.0)
        .map(|(_, plain, upper)| match shifted {
            true => *upper,
            false => *plain,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Modifiers = Modifiers {
        ctrl: false,
        alt: false,
        shift: false,
        win: false,
    };

    fn shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..NONE
        }
    }

    fn ctrl() -> Modifiers {
        Modifiers { ctrl: true, ..NONE }
    }

    #[test]
    fn a_letter_types_lower_case_until_shifted() {
        assert_eq!(printable(keys::W, NONE), Some('w'));
        assert_eq!(printable(keys::W, shift()), Some('W'));
    }

    #[test]
    fn a_digit_shifts_to_its_symbol() {
        assert_eq!(printable(keys::DIGIT_1, NONE), Some('1'));
        assert_eq!(printable(keys::DIGIT_1, shift()), Some('!'));
    }

    #[test]
    fn space_types_a_space() {
        assert_eq!(printable(keys::SPACE, NONE), Some(' '));
    }

    /// A path needs these, so they are the point of the punctuation table.
    #[test]
    fn the_keys_a_path_needs_all_type() {
        assert_eq!(printable(KeyCode(0xDC), NONE), Some('\\'));
        assert_eq!(printable(KeyCode(0xBA), shift()), Some(':'));
        assert_eq!(printable(KeyCode(0xBD), NONE), Some('-'));
        assert_eq!(printable(KeyCode(0xBE), NONE), Some('.'));
        assert_eq!(printable(KeyCode(0xBF), NONE), Some('/'));
        assert_eq!(printable(KeyCode(0xBD), shift()), Some('_'));
    }

    #[test]
    fn a_url_can_be_typed() {
        let typed: String = [
            (KeyCode(b'H' as u16), NONE),
            (KeyCode(0xBA), shift()),
            (KeyCode(0xBF), NONE),
            (KeyCode(0xBF), NONE),
        ]
        .into_iter()
        .filter_map(|(code, modifiers)| printable(code, modifiers))
        .collect();

        assert_eq!(typed, "h://");
    }

    #[test]
    fn a_combo_types_nothing() {
        assert_eq!(printable(keys::V, ctrl()), None);
        assert_eq!(printable(keys::W, ctrl()), None);
    }

    #[test]
    fn the_keys_the_form_uses_type_nothing() {
        assert_eq!(printable(keys::ENTER, NONE), None);
        assert_eq!(printable(keys::TAB, NONE), None);
        assert_eq!(printable(keys::BACKSPACE, NONE), None);
        assert_eq!(printable(keys::ARROW_LEFT, NONE), None);
        assert_eq!(printable(keys::DELETE, NONE), None);
        assert_eq!(printable(keys::HOME, NONE), None);
    }

    #[test]
    fn every_shifted_digit_is_listed() {
        for digit in 0..10u8 {
            let code = KeyCode(u16::from(b'0' + digit));

            assert_eq!(
                printable(code, shift()),
                Some(SHIFTED_DIGITS[usize::from(digit)])
            );
        }
    }
}
