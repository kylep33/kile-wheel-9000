//! macOS virtual key codes (Carbon's kVK_*) to the Win32 codes the config speaks.

use kile_wheel_core::binding::KeyCode;

/// `(kVK code, VK code)`. Command is the Win modifier and Option is Alt.
const TABLE: &[(u16, u16)] = &[
    (0, 0x41),   // A
    (1, 0x53),   // S
    (2, 0x44),   // D
    (3, 0x46),   // F
    (4, 0x48),   // H
    (5, 0x47),   // G
    (6, 0x5A),   // Z
    (7, 0x58),   // X
    (8, 0x43),   // C
    (9, 0x56),   // V
    (11, 0x42),  // B
    (12, 0x51),  // Q
    (13, 0x57),  // W
    (14, 0x45),  // E
    (15, 0x52),  // R
    (16, 0x59),  // Y
    (17, 0x54),  // T
    (18, 0x31),  // 1
    (19, 0x32),  // 2
    (20, 0x33),  // 3
    (21, 0x34),  // 4
    (22, 0x36),  // 6
    (23, 0x35),  // 5
    (24, 0xBB),  // EQUAL
    (25, 0x39),  // 9
    (26, 0x37),  // 7
    (27, 0xBD),  // MINUS
    (28, 0x38),  // 8
    (29, 0x30),  // 0
    (30, 0xDD),  // RIGHT BRACKET
    (31, 0x4F),  // O
    (32, 0x55),  // U
    (33, 0xDB),  // LEFT BRACKET
    (34, 0x49),  // I
    (35, 0x50),  // P
    (36, 0x0D),  // RETURN
    (37, 0x4C),  // L
    (38, 0x4A),  // J
    (39, 0xDE),  // QUOTE
    (40, 0x4B),  // K
    (41, 0xBA),  // SEMICOLON
    (42, 0xDC),  // BACKSLASH
    (43, 0xBC),  // COMMA
    (44, 0xBF),  // SLASH
    (45, 0x4E),  // N
    (46, 0x4D),  // M
    (47, 0xBE),  // PERIOD
    (48, 0x09),  // TAB
    (49, 0x20),  // SPACE
    (50, 0xC0),  // GRAVE
    (51, 0x08),  // DELETE, which is backspace
    (53, 0x1B),  // ESCAPE
    (54, 0x5C),  // RIGHT COMMAND
    (55, 0x5B),  // COMMAND
    (56, 0xA0),  // SHIFT
    (57, 0x14),  // CAPS LOCK
    (58, 0xA4),  // OPTION
    (59, 0xA2),  // CONTROL
    (60, 0xA1),  // RIGHT SHIFT
    (61, 0xA5),  // RIGHT OPTION
    (62, 0xA3),  // RIGHT CONTROL
    (64, 0x80),  // F17
    (65, 0x6E),  // KEYPAD DECIMAL
    (67, 0x6A),  // KEYPAD MULTIPLY
    (69, 0x6B),  // KEYPAD PLUS
    (75, 0x6F),  // KEYPAD DIVIDE
    (76, 0x0D),  // KEYPAD ENTER
    (78, 0x6D),  // KEYPAD MINUS
    (79, 0x81),  // F18
    (80, 0x82),  // F19
    (82, 0x60),  // KEYPAD 0
    (83, 0x61),
    (84, 0x62),
    (85, 0x63),
    (86, 0x64),
    (87, 0x65),
    (88, 0x66),
    (89, 0x67),  // KEYPAD 7
    (90, 0x83),  // F20
    (91, 0x68),  // KEYPAD 8
    (92, 0x69),  // KEYPAD 9
    (96, 0x74),  // F5
    (97, 0x75),  // F6
    (98, 0x76),  // F7
    (99, 0x72),  // F3
    (100, 0x77), // F8
    (101, 0x78), // F9
    (103, 0x7A), // F11
    (105, 0x7C), // F13
    (106, 0x7F), // F16
    (107, 0x7D), // F14
    (109, 0x79), // F10
    (111, 0x7B), // F12
    (113, 0x7E), // F15
    (114, 0x2D), // HELP, where Insert sits on a PC layout
    (115, 0x24), // HOME
    (116, 0x21), // PAGE UP
    (117, 0x2E), // FORWARD DELETE
    (118, 0x73), // F4
    (119, 0x23), // END
    (120, 0x71), // F2
    (121, 0x22), // PAGE DOWN
    (122, 0x70), // F1
    (123, 0x25), // LEFT
    (124, 0x27), // RIGHT
    (125, 0x28), // DOWN
    (126, 0x26), // UP
];

pub fn to_vk(code: u16) -> Option<KeyCode> {
    TABLE
        .iter()
        .find(|(mac, _)| *mac == code)
        .map(|(_, vk)| KeyCode(*vk))
}

/// Only the tests need the reverse direction; sending uses fixed key codes.
#[cfg(test)]
pub fn from_vk(key: KeyCode) -> Option<u16> {
    TABLE
        .iter()
        .find(|(_, vk)| *vk == key.0)
        .map(|(mac, _)| *mac)
}
