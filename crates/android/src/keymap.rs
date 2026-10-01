//! Android `KeyEvent` keycodes (a hardware keyboard on the phone or tablet)
//! → macOS virtual keycodes (Carbon `kVK_*`), what the wire carries.

/// The Mac keycode for an Android keycode, if the key has one.
pub fn mac_keycode(android: i32) -> Option<u16> {
    // Letters: Android numbers them A-Z; Carbon follows the physical layout.
    const LETTERS: [u16; 26] = [
        0x00, 0x0B, 0x08, 0x02, 0x0E, 0x03, 0x05, 0x04, 0x22, 0x26, 0x28, 0x25, 0x2E, 0x2D, 0x1F, 0x23, 0x0C, 0x0F, 0x01,
        0x11, 0x20, 0x09, 0x0D, 0x07, 0x10, 0x06,
    ];
    const DIGITS: [u16; 10] = [0x1D, 0x12, 0x13, 0x14, 0x15, 0x17, 0x16, 0x1A, 0x1C, 0x19];
    const KEYPAD: [u16; 10] = [0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5B, 0x5C];
    const FUNCTION: [u16; 12] = [0x7A, 0x78, 0x63, 0x76, 0x60, 0x61, 0x62, 0x64, 0x65, 0x6D, 0x67, 0x6F];
    let code = match android {
        7..=16 => DIGITS[(android - 7) as usize],
        29..=54 => LETTERS[(android - 29) as usize],
        131..=142 => FUNCTION[(android - 131) as usize],
        144..=153 => KEYPAD[(android - 144) as usize],
        19 => 0x7E,  // DPAD_UP
        20 => 0x7D,  // DPAD_DOWN
        21 => 0x7B,  // DPAD_LEFT
        22 => 0x7C,  // DPAD_RIGHT
        55 => 0x2B,  // COMMA
        56 => 0x2F,  // PERIOD
        57 => 0x3A,  // ALT_LEFT
        58 => 0x3D,  // ALT_RIGHT
        59 => 0x38,  // SHIFT_LEFT
        60 => 0x3C,  // SHIFT_RIGHT
        61 => 0x30,  // TAB
        62 => 0x31,  // SPACE
        66 => 0x24,  // ENTER
        67 => 0x33,  // DEL (backspace)
        68 => 0x32,  // GRAVE
        69 => 0x1B,  // MINUS
        70 => 0x18,  // EQUALS
        71 => 0x21,  // LEFT_BRACKET
        72 => 0x1E,  // RIGHT_BRACKET
        73 => 0x2A,  // BACKSLASH
        74 => 0x29,  // SEMICOLON
        75 => 0x27,  // APOSTROPHE
        76 => 0x2C,  // SLASH
        92 => 0x74,  // PAGE_UP
        93 => 0x79,  // PAGE_DOWN
        111 => 0x35, // ESCAPE
        112 => 0x75, // FORWARD_DEL
        113 => 0x3B, // CTRL_LEFT
        114 => 0x3E, // CTRL_RIGHT
        115 => 0x39, // CAPS_LOCK
        117 => 0x37, // META_LEFT
        118 => 0x36, // META_RIGHT
        122 => 0x73, // MOVE_HOME
        123 => 0x77, // MOVE_END
        124 => 0x72, // INSERT (the Help key's place)
        154 => 0x4B, // NUMPAD_DIVIDE
        155 => 0x43, // NUMPAD_MULTIPLY
        156 => 0x4E, // NUMPAD_SUBTRACT
        157 => 0x45, // NUMPAD_ADD
        158 => 0x41, // NUMPAD_DOT
        160 => 0x4C, // NUMPAD_ENTER
        161 => 0x51, // NUMPAD_EQUALS
        _ => return None,
    };
    Some(code)
}

/// A Mac host gets ⌘ for Ctrl and ⌃ for the Meta key, so Ctrl+C copies
/// there as it does here (as the Linux viewer does).
pub fn for_host(code: u16, host_is_mac: bool) -> u16 {
    const COMMAND: u16 = 0x37;
    const RIGHT_COMMAND: u16 = 0x36;
    const CONTROL: u16 = 0x3B;
    const RIGHT_CONTROL: u16 = 0x3E;
    if !host_is_mac {
        return code;
    }
    match code {
        CONTROL => COMMAND,
        RIGHT_CONTROL => RIGHT_COMMAND,
        COMMAND => CONTROL,
        RIGHT_COMMAND => RIGHT_CONTROL,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_land_where_a_us_keyboard_has_them() {
        assert_eq!(mac_keycode(29), Some(0x00)); // A
        assert_eq!(mac_keycode(54), Some(0x06)); // Z
        assert_eq!(mac_keycode(41), Some(0x2E)); // M
        assert_eq!(mac_keycode(7), Some(0x1D)); // 0
        assert_eq!(mac_keycode(16), Some(0x19)); // 9
        assert_eq!(mac_keycode(131), Some(0x7A)); // F1
        assert_eq!(mac_keycode(142), Some(0x6F)); // F12
        assert_eq!(mac_keycode(67), Some(0x33)); // backspace
        assert_eq!(mac_keycode(24), None); // volume up stays the phone's
    }

    #[test]
    fn letters_agree_with_typed_text() {
        for (i, c) in ('a'..='z').enumerate() {
            let typed = sunna_client::keys::us_key(c).map(|(code, _)| code);
            assert_eq!(mac_keycode(29 + i as i32), typed, "{c}");
        }
        for (i, c) in ('0'..='9').enumerate() {
            let typed = sunna_client::keys::us_key(c).map(|(code, _)| code);
            assert_eq!(mac_keycode(7 + i as i32), typed, "{c}");
        }
    }

    #[test]
    fn ctrl_is_command_on_a_mac() {
        assert_eq!(for_host(0x3B, true), 0x37);
        assert_eq!(for_host(0x37, true), 0x3B);
        assert_eq!(for_host(0x3B, false), 0x3B);
    }
}
