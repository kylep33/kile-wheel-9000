//! Linux key codes to the Win32 virtual-key codes the config and the wheel speak.
//!
//! Evdev codes name physical keys, so this is the US layout's reading of them, the same
//! assumption the wheel's own typing makes.

use kile_wheel_core::binding::KeyCode;

/// `(evdev code, VK code)`, from linux/input-event-codes.h and the Win32 VK table.
const TABLE: &[(u16, u16)] = &[
    (1, 0x1B),   // ESC
    (2, 0x31),   // 1
    (3, 0x32),
    (4, 0x33),
    (5, 0x34),
    (6, 0x35),
    (7, 0x36),
    (8, 0x37),
    (9, 0x38),
    (10, 0x39),  // 9
    (11, 0x30),  // 0
    (12, 0xBD),  // MINUS
    (13, 0xBB),  // EQUAL
    (14, 0x08),  // BACKSPACE
    (15, 0x09),  // TAB
    (16, 0x51),  // Q
    (17, 0x57),  // W
    (18, 0x45),  // E
    (19, 0x52),  // R
    (20, 0x54),  // T
    (21, 0x59),  // Y
    (22, 0x55),  // U
    (23, 0x49),  // I
    (24, 0x4F),  // O
    (25, 0x50),  // P
    (26, 0xDB),  // LEFTBRACE
    (27, 0xDD),  // RIGHTBRACE
    (28, 0x0D),  // ENTER
    (29, 0xA2),  // LEFTCTRL
    (30, 0x41),  // A
    (31, 0x53),  // S
    (32, 0x44),  // D
    (33, 0x46),  // F
    (34, 0x47),  // G
    (35, 0x48),  // H
    (36, 0x4A),  // J
    (37, 0x4B),  // K
    (38, 0x4C),  // L
    (39, 0xBA),  // SEMICOLON
    (40, 0xDE),  // APOSTROPHE
    (41, 0xC0),  // GRAVE
    (42, 0xA0),  // LEFTSHIFT
    (43, 0xDC),  // BACKSLASH
    (44, 0x5A),  // Z
    (45, 0x58),  // X
    (46, 0x43),  // C
    (47, 0x56),  // V
    (48, 0x42),  // B
    (49, 0x4E),  // N
    (50, 0x4D),  // M
    (51, 0xBC),  // COMMA
    (52, 0xBE),  // DOT
    (53, 0xBF),  // SLASH
    (54, 0xA1),  // RIGHTSHIFT
    (55, 0x6A),  // KPASTERISK
    (56, 0xA4),  // LEFTALT
    (57, 0x20),  // SPACE
    (58, 0x14),  // CAPSLOCK
    (59, 0x70),  // F1
    (60, 0x71),
    (61, 0x72),
    (62, 0x73),
    (63, 0x74),
    (64, 0x75),
    (65, 0x76),
    (66, 0x77),
    (67, 0x78),
    (68, 0x79),  // F10
    (69, 0x90),  // NUMLOCK
    (70, 0x91),  // SCROLLLOCK
    (71, 0x67),  // KP7
    (72, 0x68),  // KP8
    (73, 0x69),  // KP9
    (74, 0x6D),  // KPMINUS
    (75, 0x64),  // KP4
    (76, 0x65),  // KP5
    (77, 0x66),  // KP6
    (78, 0x6B),  // KPPLUS
    (79, 0x61),  // KP1
    (80, 0x62),  // KP2
    (81, 0x63),  // KP3
    (82, 0x60),  // KP0
    (83, 0x6E),  // KPDOT
    (86, 0xE2),  // 102ND
    (87, 0x7A),  // F11
    (88, 0x7B),  // F12
    (96, 0x0D),  // KPENTER
    (97, 0xA3),  // RIGHTCTRL
    (98, 0x6F),  // KPSLASH
    (99, 0x2C),  // SYSRQ, print screen
    (100, 0xA5), // RIGHTALT
    (102, 0x24), // HOME
    (103, 0x26), // UP
    (104, 0x21), // PAGEUP
    (105, 0x25), // LEFT
    (106, 0x27), // RIGHT
    (107, 0x23), // END
    (108, 0x28), // DOWN
    (109, 0x22), // PAGEDOWN
    (110, 0x2D), // INSERT
    (111, 0x2E), // DELETE
    (119, 0x13), // PAUSE
    (125, 0x5B), // LEFTMETA, the Super key
    (126, 0x5C), // RIGHTMETA
    (127, 0x5D), // COMPOSE, the menu key
    (183, 0x7C), // F13
    (184, 0x7D),
    (185, 0x7E),
    (186, 0x7F),
    (187, 0x80),
    (188, 0x81),
    (189, 0x82),
    (190, 0x83),
    (191, 0x84),
    (192, 0x85),
    (193, 0x86),
    (194, 0x87), // F24
];

pub fn to_vk(code: u16) -> Option<KeyCode> {
    TABLE
        .iter()
        .find(|(evdev, _)| *evdev == code)
        .map(|(_, vk)| KeyCode(*vk))
}

/// The first evdev key for a VK, which for the keypad duplicates is the main block.
pub fn from_vk(key: KeyCode) -> Option<u16> {
    TABLE
        .iter()
        .find(|(_, vk)| *vk == key.0)
        .map(|(evdev, _)| *evdev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kile_wheel_core::binding::keys;

    const KEY_CAPSLOCK: u16 = 58;
    const KEY_W: u16 = 17;
    const KEY_LEFTMETA: u16 = 125;

    #[test]
    fn the_default_hotkey_is_recognised() {
        assert_eq!(to_vk(KEY_CAPSLOCK), Some(keys::CAPS_LOCK));
    }

    #[test]
    fn super_is_the_win_modifier() {
        assert_eq!(to_vk(KEY_LEFTMETA), Some(keys::LEFT_WIN));
    }

    #[test]
    fn a_letter_maps_both_ways() {
        assert_eq!(to_vk(KEY_W), Some(keys::W));
        assert_eq!(from_vk(keys::W), Some(KEY_W));
    }

    #[test]
    fn enter_types_with_the_main_key_not_the_keypad_one() {
        assert_eq!(from_vk(keys::ENTER), Some(28));
    }

    #[test]
    fn no_evdev_code_is_listed_twice() {
        for (index, (code, _)) in TABLE.iter().enumerate() {
            assert!(
                TABLE[index + 1..].iter().all(|(other, _)| other != code),
                "evdev {code} twice"
            );
        }
    }
}
