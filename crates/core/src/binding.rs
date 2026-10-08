//! Hotkey bindings: how they are spelled in the config and how the hook matches them.

use std::fmt;
use std::str::FromStr;

use serde::Deserialize;
use thiserror::Error;

const SEPARATOR: char = '+';
const FUNCTION_KEY_PREFIX: &str = "F";
const FUNCTION_KEY_COUNT: u16 = 24;
const FUNCTION_KEY_1: KeyCode = KeyCode(0x70);
const VK_ESCAPE_PREFIX: &str = "vk(";
const VK_ESCAPE_SUFFIX: &str = ")";
const HEX_PREFIX: &str = "0x";

/// Written in the order Windows itself uses, so `Alt+Shift` round-trips unchanged.
const MODIFIER_NAMES: [&str; 4] = ["Ctrl", "Alt", "Shift", "Win"];

/// A Win32 virtual-key code. Held numerically so an unnamed key can still bind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyCode(pub u16);

/// Win32 `VK_*` codes for the keys this project names in code.
pub mod keys {
    use super::KeyCode;

    pub const BACKSPACE: KeyCode = KeyCode(0x08);
    pub const TAB: KeyCode = KeyCode(0x09);
    pub const ENTER: KeyCode = KeyCode(0x0D);
    pub const SHIFT: KeyCode = KeyCode(0x10);
    pub const CTRL: KeyCode = KeyCode(0x11);
    pub const ALT: KeyCode = KeyCode(0x12);
    pub const PAUSE: KeyCode = KeyCode(0x13);
    pub const CAPS_LOCK: KeyCode = KeyCode(0x14);
    pub const ESC: KeyCode = KeyCode(0x1B);
    pub const SPACE: KeyCode = KeyCode(0x20);
    pub const PAGE_UP: KeyCode = KeyCode(0x21);
    pub const PAGE_DOWN: KeyCode = KeyCode(0x22);
    pub const END: KeyCode = KeyCode(0x23);
    pub const HOME: KeyCode = KeyCode(0x24);
    pub const ARROW_LEFT: KeyCode = KeyCode(0x25);
    pub const ARROW_UP: KeyCode = KeyCode(0x26);
    pub const ARROW_RIGHT: KeyCode = KeyCode(0x27);
    pub const ARROW_DOWN: KeyCode = KeyCode(0x28);
    pub const PRINT_SCREEN: KeyCode = KeyCode(0x2C);
    pub const INSERT: KeyCode = KeyCode(0x2D);
    pub const DELETE: KeyCode = KeyCode(0x2E);
    pub const LEFT_WIN: KeyCode = KeyCode(0x5B);
    pub const RIGHT_WIN: KeyCode = KeyCode(0x5C);
    pub const APPS: KeyCode = KeyCode(0x5D);
    pub const NUM_LOCK: KeyCode = KeyCode(0x90);
    pub const SCROLL_LOCK: KeyCode = KeyCode(0x91);
    pub const LEFT_SHIFT: KeyCode = KeyCode(0xA0);
    pub const RIGHT_SHIFT: KeyCode = KeyCode(0xA1);
    pub const LEFT_CTRL: KeyCode = KeyCode(0xA2);
    pub const RIGHT_CTRL: KeyCode = KeyCode(0xA3);
    pub const LEFT_ALT: KeyCode = KeyCode(0xA4);
    pub const RIGHT_ALT: KeyCode = KeyCode(0xA5);

    /// Letters and digits sit on their uppercase ascii value.
    pub const A: KeyCode = KeyCode(b'A' as u16);
    pub const D: KeyCode = KeyCode(b'D' as u16);
    pub const S: KeyCode = KeyCode(b'S' as u16);
    pub const V: KeyCode = KeyCode(b'V' as u16);
    pub const W: KeyCode = KeyCode(b'W' as u16);
    pub const DIGIT_1: KeyCode = KeyCode(b'1' as u16);
    pub const DIGIT_9: KeyCode = KeyCode(b'9' as u16);
}

/// Every key that only ever acts as a modifier, in any of its forms.
const MODIFIER_KEYS: &[KeyCode] = &[
    keys::CTRL,
    keys::ALT,
    keys::SHIFT,
    keys::LEFT_CTRL,
    keys::RIGHT_CTRL,
    keys::LEFT_ALT,
    keys::RIGHT_ALT,
    keys::LEFT_SHIFT,
    keys::RIGHT_SHIFT,
    keys::LEFT_WIN,
    keys::RIGHT_WIN,
];

pub fn is_modifier_key(key: KeyCode) -> bool {
    MODIFIER_KEYS.contains(&key)
}

/// Canonical name first; later spellings parse but never print.
const NAMED_KEYS: &[(&str, KeyCode)] = &[
    ("Backspace", keys::BACKSPACE),
    ("Tab", keys::TAB),
    ("Enter", keys::ENTER),
    ("Pause", keys::PAUSE),
    ("CapsLock", keys::CAPS_LOCK),
    ("Esc", keys::ESC),
    ("Escape", keys::ESC),
    ("Space", keys::SPACE),
    ("PageUp", keys::PAGE_UP),
    ("PageDown", keys::PAGE_DOWN),
    ("End", keys::END),
    ("Home", keys::HOME),
    ("Left", keys::ARROW_LEFT),
    ("Up", keys::ARROW_UP),
    ("Right", keys::ARROW_RIGHT),
    ("Down", keys::ARROW_DOWN),
    ("PrintScreen", keys::PRINT_SCREEN),
    ("Insert", keys::INSERT),
    ("Delete", keys::DELETE),
    ("Apps", keys::APPS),
    ("NumLock", keys::NUM_LOCK),
    ("ScrollLock", keys::SCROLL_LOCK),
];

/// Mouse buttons a binding can use, including the two side buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

const NAMED_MOUSE_BUTTONS: &[(&str, MouseButton)] = &[
    ("MouseLeft", MouseButton::Left),
    ("MouseRight", MouseButton::Right),
    ("MouseMiddle", MouseButton::Middle),
    ("XButton1", MouseButton::X1),
    ("XButton2", MouseButton::X2),
];

impl MouseButton {
    fn name(self) -> &'static str {
        NAMED_MOUSE_BUTTONS
            .iter()
            .find(|(_, candidate)| *candidate == self)
            .map(|(name, _)| *name)
            .expect("every mouse button has a table entry")
    }
}

/// What a binding fires on, beyond the modifiers it needs held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trigger {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// Modifiers that must be held, exactly. Shift+CapsLock therefore misses a bare
/// CapsLock binding, which is what lets real Caps Lock through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl Modifiers {
    pub fn any(self) -> bool {
        self.ctrl || self.alt || self.shift || self.win
    }

    fn held(self) -> [bool; MODIFIER_NAMES.len()] {
        [self.ctrl, self.alt, self.shift, self.win]
    }
}

/// A key, combo, or mouse button that the hook watches for.
///
/// `trigger` is `None` for a modifier-only binding such as `Alt+Shift`, which fires on
/// whichever of its modifiers completes the set. That makes press order irrelevant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Binding {
    pub modifiers: Modifiers,
    pub trigger: Option<Trigger>,
}

impl Binding {
    pub fn key(trigger: KeyCode) -> Self {
        Self {
            modifiers: Modifiers::default(),
            trigger: Some(Trigger::Key(trigger)),
        }
    }

    /// The binding a key press should be looked up as.
    ///
    /// `held` already includes the key being pressed, so a modifier press can only ever
    /// complete a modifier-only combo.
    pub fn for_key_press(key: KeyCode, held: Modifiers) -> Self {
        let trigger = (!is_modifier_key(key)).then_some(Trigger::Key(key));

        Self {
            modifiers: held,
            trigger,
        }
    }

    pub fn for_button_press(button: MouseButton, held: Modifiers) -> Self {
        Self {
            modifiers: held,
            trigger: Some(Trigger::Mouse(button)),
        }
    }
}

/// What a matched binding does to the wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Open,
    Close,
    Toggle,
}

impl Action {
    /// A close only means something while the wheel is open. Without this, binding Esc
    /// to close would swallow Esc in every other app.
    fn applies(self, wheel_open: bool) -> bool {
        match self {
            Self::Close => wheel_open,
            Self::Open | Self::Toggle => true,
        }
    }
}

/// One config entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub binding: Binding,
    pub action: Action,
}

/// Bindings compiled once on config load, so the hook path only looks up.
#[derive(Debug, Clone, Default)]
pub struct BindingTable {
    hotkeys: Vec<Hotkey>,
}

impl BindingTable {
    pub fn new(hotkeys: Vec<Hotkey>) -> Self {
        Self { hotkeys }
    }

    /// Scans linearly because there are only ever a handful of bindings, which beats
    /// hashing and allocates nothing on the hook path.
    pub fn action_for(&self, binding: Binding, wheel_open: bool) -> Option<Action> {
        self.hotkeys
            .iter()
            .find(|hotkey| hotkey.binding == binding && hotkey.action.applies(wheel_open))
            .map(|hotkey| hotkey.action)
    }

    pub fn hotkeys(&self) -> &[Hotkey] {
        &self.hotkeys
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseBindingError {
    #[error("binding is empty, so nothing would fire it; write a key name such as \"CapsLock\"")]
    Empty,

    #[error("binding \"{0}\" has a blank part; remove the stray '+'")]
    BlankPart(String),

    #[error(
        "\"{0}\" is not a known key or mouse button; use a name such as \"CapsLock\", \"Alt+Shift\", \"F13\", \"XButton1\", or \"vk(0x5B)\" for an unnamed key"
    )]
    UnknownKey(String),

    #[error(
        "a mouse button has to be the last part of \"{0}\"; write modifiers first, as in \"Ctrl+XButton1\""
    )]
    MouseNotLast(String),
}

impl FromStr for Binding {
    type Err = ParseBindingError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let whole = text.trim();
        if whole.is_empty() {
            return Err(ParseBindingError::Empty);
        }

        let parts: Vec<&str> = whole.split(SEPARATOR).map(str::trim).collect();
        if parts.iter().any(|part| part.is_empty()) {
            return Err(ParseBindingError::BlankPart(whole.to_owned()));
        }

        // all modifiers means the combo itself is the binding, with no distinct trigger
        if let Some(modifiers) = all_modifiers(&parts) {
            return Ok(Self {
                modifiers,
                trigger: None,
            });
        }

        let (last, leading) = parts.split_last().expect("split always yields a part");
        let trigger =
            parse_trigger(last).ok_or_else(|| ParseBindingError::UnknownKey((*last).to_owned()))?;

        Ok(Self {
            modifiers: parse_modifiers(leading, whole)?,
            trigger: Some(trigger),
        })
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut written = false;
        for (held, name) in self.modifiers.held().into_iter().zip(MODIFIER_NAMES) {
            if !held {
                continue;
            }
            if written {
                formatter.write_fmt(format_args!("{SEPARATOR}"))?;
            }
            formatter.write_str(name)?;
            written = true;
        }

        let Some(trigger) = self.trigger else {
            return Ok(());
        };
        if written {
            formatter.write_fmt(format_args!("{SEPARATOR}"))?;
        }

        match trigger {
            Trigger::Mouse(button) => formatter.write_str(button.name()),
            Trigger::Key(code) => write!(formatter, "{code}"),
        }
    }
}

impl fmt::Display for KeyCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = named_key_name(*self) {
            return formatter.write_str(name);
        }
        if let Some(number) = function_key_number(*self) {
            return write!(formatter, "{FUNCTION_KEY_PREFIX}{number}");
        }
        if let Some(character) = character_key(*self) {
            return write!(formatter, "{character}");
        }
        write!(
            formatter,
            "{VK_ESCAPE_PREFIX}{HEX_PREFIX}{:02X}{VK_ESCAPE_SUFFIX}",
            self.0
        )
    }
}

/// `Some` only when every part names a modifier.
fn all_modifiers(parts: &[&str]) -> Option<Modifiers> {
    let mut modifiers = Modifiers::default();
    for part in parts {
        if !apply_modifier(&mut modifiers, part) {
            return None;
        }
    }
    Some(modifiers)
}

fn parse_modifiers(parts: &[&str], whole: &str) -> Result<Modifiers, ParseBindingError> {
    let mut modifiers = Modifiers::default();
    for part in parts {
        if named_mouse_button(part).is_some() {
            return Err(ParseBindingError::MouseNotLast(whole.to_owned()));
        }
        if !apply_modifier(&mut modifiers, part) {
            return Err(ParseBindingError::UnknownKey((*part).to_owned()));
        }
    }
    Ok(modifiers)
}

/// Reports whether the name was a modifier.
fn apply_modifier(modifiers: &mut Modifiers, name: &str) -> bool {
    match name.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => modifiers.ctrl = true,
        "alt" => modifiers.alt = true,
        "shift" => modifiers.shift = true,
        "win" | "super" | "meta" => modifiers.win = true,
        _ => return false,
    }
    true
}

fn parse_trigger(token: &str) -> Option<Trigger> {
    if let Some(button) = named_mouse_button(token) {
        return Some(Trigger::Mouse(button));
    }
    parse_key(token).map(Trigger::Key)
}

fn parse_key(token: &str) -> Option<KeyCode> {
    named_key(token)
        .or_else(|| function_key(token))
        .or_else(|| character_key_code(token))
        .or_else(|| explicit_vk(token))
}

fn named_mouse_button(token: &str) -> Option<MouseButton> {
    NAMED_MOUSE_BUTTONS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(token))
        .map(|(_, button)| *button)
}

fn named_key(token: &str) -> Option<KeyCode> {
    NAMED_KEYS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(token))
        .map(|(_, code)| *code)
}

fn named_key_name(code: KeyCode) -> Option<&'static str> {
    NAMED_KEYS
        .iter()
        .find(|(_, candidate)| *candidate == code)
        .map(|(name, _)| *name)
}

fn function_key(token: &str) -> Option<KeyCode> {
    let (prefix, digits) = token.split_at_checked(FUNCTION_KEY_PREFIX.len())?;
    if !prefix.eq_ignore_ascii_case(FUNCTION_KEY_PREFIX) {
        return None;
    }

    let number: u16 = digits.parse().ok()?;
    if number == 0 || number > FUNCTION_KEY_COUNT {
        return None;
    }
    Some(KeyCode(FUNCTION_KEY_1.0 + number - 1))
}

fn function_key_number(code: KeyCode) -> Option<u16> {
    let offset = code.0.checked_sub(FUNCTION_KEY_1.0)?;
    (offset < FUNCTION_KEY_COUNT).then_some(offset + 1)
}

fn character_key_code(token: &str) -> Option<KeyCode> {
    let mut characters = token.chars();
    let single = characters.next()?;
    if characters.next().is_some() || !single.is_ascii_alphanumeric() {
        return None;
    }
    Some(KeyCode(u16::from(single.to_ascii_uppercase() as u8)))
}

fn character_key(code: KeyCode) -> Option<char> {
    let character = char::from(u8::try_from(code.0).ok()?);
    character.is_ascii_alphanumeric().then_some(character)
}

fn explicit_vk(token: &str) -> Option<KeyCode> {
    let lowered = token.to_ascii_lowercase();
    let inner = lowered
        .strip_prefix(VK_ESCAPE_PREFIX)?
        .strip_suffix(VK_ESCAPE_SUFFIX)?;

    match inner.strip_prefix(HEX_PREFIX) {
        Some(hex) => u16::from_str_radix(hex, 16).ok().map(KeyCode),
        None => inner.parse().ok().map(KeyCode),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Binding {
        text.parse().expect("binding should parse")
    }

    #[test]
    fn a_bare_key_name_parses_without_modifiers() {
        let binding = parse("CapsLock");

        assert_eq!(binding.trigger, Some(Trigger::Key(keys::CAPS_LOCK)));
        assert_eq!(binding.modifiers, Modifiers::default());
    }

    #[test]
    fn modifiers_parse_in_any_order_and_case() {
        let expected = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::default()
        };

        assert_eq!(parse("ctrl+SHIFT+k").modifiers, expected);
        assert_eq!(parse("Shift+Ctrl+K").modifiers, expected);
    }

    #[test]
    fn a_modifier_only_combo_has_no_trigger() {
        let binding = parse("Alt+Shift");

        assert_eq!(binding.trigger, None);
        assert!(binding.modifiers.alt && binding.modifiers.shift);
    }

    #[test]
    fn a_modifier_only_combo_matches_in_either_press_order() {
        let both = Modifiers {
            alt: true,
            shift: true,
            ..Modifiers::default()
        };

        // whichever modifier lands last is the one that completes the set
        assert_eq!(
            Binding::for_key_press(keys::LEFT_SHIFT, both),
            parse("Alt+Shift")
        );
        assert_eq!(
            Binding::for_key_press(keys::RIGHT_ALT, both),
            parse("Alt+Shift")
        );
    }

    #[test]
    fn letters_digits_and_function_keys_map_to_their_codes() {
        assert_eq!(parse("K").trigger, Some(Trigger::Key(KeyCode(b'K' as u16))));
        assert_eq!(parse("7").trigger, Some(Trigger::Key(KeyCode(b'7' as u16))));
        assert_eq!(parse("F13").trigger, Some(Trigger::Key(KeyCode(0x7C))));
        assert_eq!(parse("f24").trigger, Some(Trigger::Key(KeyCode(0x87))));
    }

    #[test]
    fn mouse_buttons_parse_as_triggers() {
        assert_eq!(
            parse("XButton1").trigger,
            Some(Trigger::Mouse(MouseButton::X1))
        );
        assert_eq!(
            parse("Ctrl+MouseMiddle").trigger,
            Some(Trigger::Mouse(MouseButton::Middle))
        );
    }

    #[test]
    fn an_unnamed_key_binds_through_the_vk_escape() {
        assert_eq!(
            parse("vk(0x14)").trigger,
            Some(Trigger::Key(keys::CAPS_LOCK))
        );
        assert_eq!(parse("VK(20)").trigger, Some(Trigger::Key(keys::CAPS_LOCK)));
    }

    #[test]
    fn every_form_round_trips_through_display() {
        for text in [
            "CapsLock",
            "Alt+Shift",
            "Ctrl+Alt+Shift+Win",
            "Ctrl+Shift+K",
            "Alt+F13",
            "Win+XButton2",
            "7",
        ] {
            assert_eq!(parse(text).to_string(), text);
        }
    }

    #[test]
    fn an_unnamed_code_prints_as_a_vk_escape() {
        let binding = Binding::key(KeyCode(0xFF));

        assert_eq!(binding.to_string(), "vk(0xFF)");
    }

    #[test]
    fn bad_bindings_report_what_is_wrong() {
        assert_eq!("".parse::<Binding>(), Err(ParseBindingError::Empty));
        assert_eq!(
            "Ctrl+".parse::<Binding>(),
            Err(ParseBindingError::BlankPart("Ctrl+".to_owned()))
        );
        assert_eq!(
            "Nope".parse::<Binding>(),
            Err(ParseBindingError::UnknownKey("Nope".to_owned()))
        );
        assert_eq!(
            "F25".parse::<Binding>(),
            Err(ParseBindingError::UnknownKey("F25".to_owned()))
        );
        assert_eq!(
            "XButton1+K".parse::<Binding>(),
            Err(ParseBindingError::MouseNotLast("XButton1+K".to_owned()))
        );
    }

    #[test]
    fn exact_modifiers_keep_shift_capslock_off_a_capslock_binding() {
        let table = BindingTable::new(vec![Hotkey {
            binding: Binding::key(keys::CAPS_LOCK),
            action: Action::Toggle,
        }]);
        let shift_held = Modifiers {
            shift: true,
            ..Modifiers::default()
        };

        assert_eq!(
            table.action_for(
                Binding::for_key_press(keys::CAPS_LOCK, Modifiers::default()),
                false
            ),
            Some(Action::Toggle)
        );
        assert_eq!(
            table.action_for(Binding::for_key_press(keys::CAPS_LOCK, shift_held), false),
            None
        );
    }

    #[test]
    fn a_close_binding_is_ignored_while_the_wheel_is_shut() {
        let table = BindingTable::new(vec![Hotkey {
            binding: Binding::key(keys::ESC),
            action: Action::Close,
        }]);
        let pressed = Binding::for_key_press(keys::ESC, Modifiers::default());

        assert_eq!(table.action_for(pressed, false), None);
        assert_eq!(table.action_for(pressed, true), Some(Action::Close));
    }
}
