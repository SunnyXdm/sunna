//! Keys the viewer sends for the user from its menu: shortcuts this Mac
//! would otherwise keep for itself (⌘Tab, Spotlight...), and text typed out
//! key by key for places a paste can't reach (login screens, password
//! prompts). Keycodes are Carbon virtual keycodes, as on the wire; a Linux
//! host maps ⌘ to Super, ⌥ to Alt and ⌃ to Ctrl.

// The menu that uses these is macOS-only; the tables are tested anywhere.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use sunna_proto::messages::InputEvent;

const A: u16 = 0x00;
const L: u16 = 0x25;
const Q: u16 = 0x0C;
const T: u16 = 0x11;
const RETURN: u16 = 0x24;
const TAB: u16 = 0x30;
const SPACE: u16 = 0x31;
const ESCAPE: u16 = 0x35;
const COMMAND: u16 = 0x37;
const SHIFT: u16 = 0x38;
const OPTION: u16 = 0x3A;
const CONTROL: u16 = 0x3B;
const FORWARD_DELETE: u16 = 0x75;
const UP: u16 = 0x7E;

/// A shortcut: its keys pressed in order, then released in reverse.
pub struct Shortcut {
    pub title: &'static str,
    pub keys: &'static [u16],
}

const MAC: &[Shortcut] = &[
    Shortcut {
        title: "Switch Apps (⌘Tab)",
        keys: &[COMMAND, TAB],
    },
    Shortcut {
        title: "Spotlight (⌘Space)",
        keys: &[COMMAND, SPACE],
    },
    Shortcut {
        title: "Mission Control (⌃↑)",
        keys: &[CONTROL, UP],
    },
    Shortcut {
        title: "Force Quit… (⌥⌘Esc)",
        keys: &[COMMAND, OPTION, ESCAPE],
    },
    Shortcut {
        title: "Lock Screen (⌃⌘Q)",
        keys: &[CONTROL, COMMAND, Q],
    },
    Shortcut {
        title: "Escape",
        keys: &[ESCAPE],
    },
];

const LINUX: &[Shortcut] = &[
    Shortcut {
        title: "Switch Windows (Alt+Tab)",
        keys: &[OPTION, TAB],
    },
    Shortcut {
        title: "App Launcher (Super)",
        keys: &[COMMAND],
    },
    Shortcut {
        title: "Terminal (Ctrl+Alt+T)",
        keys: &[CONTROL, OPTION, T],
    },
    Shortcut {
        title: "Lock Screen (Super+L)",
        keys: &[COMMAND, L],
    },
    Shortcut {
        title: "Log Out… (Ctrl+Alt+Delete)",
        keys: &[CONTROL, OPTION, FORWARD_DELETE],
    },
    Shortcut {
        title: "Escape",
        keys: &[ESCAPE],
    },
];

/// The shortcuts that make sense on a host running `os` ("macOS 26.0",
/// "Arch Linux"; empty when unknown, most likely an older Mac host).
/// The host is a Mac (an unknown host is treated as one, as the menu does).
pub fn host_is_mac(os: &str) -> bool {
    let os = os.to_lowercase();
    os.is_empty() || os.contains("mac")
}

pub fn shortcuts_for(os: &str) -> &'static [Shortcut] {
    let os = os.to_lowercase();
    if os.is_empty() || os.contains("mac") {
        MAC
    } else {
        LINUX
    }
}

fn key(scancode: u16, pressed: bool) -> InputEvent {
    InputEvent::Key {
        scancode,
        pressed,
        repeat: false,
    }
}

/// Press `keys` in order, then release them in reverse.
pub fn press(keys: &[u16]) -> Vec<InputEvent> {
    let mut events: Vec<InputEvent> = keys.iter().map(|&scancode| key(scancode, true)).collect();
    events.extend(keys.iter().rev().map(|&scancode| key(scancode, false)));
    events
}

/// Where `c` is on a US keyboard, and whether it needs Shift.
pub fn us_key(c: char) -> Option<(u16, bool)> {
    // Letters: Carbon keycodes follow the physical layout, not the alphabet.
    const LETTERS: [u16; 26] = [
        A, 0x0B, 0x08, 0x02, 0x0E, 0x03, 0x05, 0x04, 0x22, 0x26, 0x28, L, 0x2E, 0x2D, 0x1F, 0x23,
        Q, 0x0F, 0x01, T, 0x20, 0x09, 0x0D, 0x07, 0x10, 0x06,
    ];
    if c.is_ascii_lowercase() {
        return Some((LETTERS[(c as u8 - b'a') as usize], false));
    }
    if c.is_ascii_uppercase() {
        return Some((LETTERS[(c as u8 - b'A') as usize], true));
    }
    let plain = |code| Some((code, false));
    let shifted = |code| Some((code, true));
    match c {
        '1' => plain(0x12),
        '2' => plain(0x13),
        '3' => plain(0x14),
        '4' => plain(0x15),
        '5' => plain(0x17),
        '6' => plain(0x16),
        '7' => plain(0x1A),
        '8' => plain(0x1C),
        '9' => plain(0x19),
        '0' => plain(0x1D),
        '!' => shifted(0x12),
        '@' => shifted(0x13),
        '#' => shifted(0x14),
        '$' => shifted(0x15),
        '%' => shifted(0x17),
        '^' => shifted(0x16),
        '&' => shifted(0x1A),
        '*' => shifted(0x1C),
        '(' => shifted(0x19),
        ')' => shifted(0x1D),
        '-' => plain(0x1B),
        '_' => shifted(0x1B),
        '=' => plain(0x18),
        '+' => shifted(0x18),
        '[' => plain(0x21),
        '{' => shifted(0x21),
        ']' => plain(0x1E),
        '}' => shifted(0x1E),
        '\\' => plain(0x2A),
        '|' => shifted(0x2A),
        ';' => plain(0x29),
        ':' => shifted(0x29),
        '\'' => plain(0x27),
        '"' => shifted(0x27),
        ',' => plain(0x2B),
        '<' => shifted(0x2B),
        '.' => plain(0x2F),
        '>' => shifted(0x2F),
        '/' => plain(0x2C),
        '?' => shifted(0x2C),
        '`' => plain(0x32),
        '~' => shifted(0x32),
        ' ' => plain(SPACE),
        '\t' => plain(TAB),
        '\n' => plain(RETURN),
        _ => None,
    }
}

/// The key presses that type `text` on a US layout, one group per
/// character (so they can be paced), and how many characters had no key.
pub fn type_text(text: &str) -> (Vec<Vec<InputEvent>>, usize) {
    let mut groups = Vec::new();
    let mut skipped = 0;
    // A Windows line ending is one Return.
    for c in text.replace("\r\n", "\n").chars() {
        match us_key(c) {
            Some((code, true)) => groups.push(press(&[SHIFT, code])),
            Some((code, false)) => groups.push(press(&[code])),
            None => skipped += 1,
        }
    }
    (groups, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(events: &[InputEvent]) -> Vec<(u16, bool)> {
        events
            .iter()
            .map(|event| match event {
                InputEvent::Key {
                    scancode, pressed, ..
                } => (*scancode, *pressed),
                other => panic!("not a key: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn shortcuts_press_then_release_in_reverse() {
        assert_eq!(
            keys(&press(&[COMMAND, OPTION, ESCAPE])),
            [
                (COMMAND, true),
                (OPTION, true),
                (ESCAPE, true),
                (ESCAPE, false),
                (OPTION, false),
                (COMMAND, false)
            ]
        );
    }

    #[test]
    fn shortcuts_follow_the_host() {
        assert_eq!(shortcuts_for("macOS 26.0")[0].title, "Switch Apps (⌘Tab)");
        assert_eq!(shortcuts_for("")[0].title, "Switch Apps (⌘Tab)");
        assert_eq!(
            shortcuts_for("Arch Linux")[0].title,
            "Switch Windows (Alt+Tab)"
        );
    }

    #[test]
    fn typing_uses_the_us_layout() {
        assert_eq!(us_key('a'), Some((A, false)));
        assert_eq!(us_key('Q'), Some((Q, true)));
        assert_eq!(us_key('m'), Some((0x2E, false)));
        assert_eq!(us_key('!'), Some((0x12, true)));
        assert_eq!(us_key('é'), None);
        let (groups, skipped) = type_text("Hi!\r\né");
        assert_eq!(skipped, 1);
        assert_eq!(groups.len(), 4);
        assert_eq!(
            keys(&groups[0]),
            [(SHIFT, true), (0x04, true), (0x04, false), (SHIFT, false)]
        );
        assert_eq!(keys(&groups[3]), [(RETURN, true), (RETURN, false)]);
    }
}
