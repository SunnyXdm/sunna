//! Pure translation shared by the Windows backend and native tests.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Scan {
    pub code: u16,
    pub extended: bool,
}

/// Carbon virtual keys to PC set-1 scan codes; E0 is a separate flag.
pub(crate) fn scan_from_mac(vk: u16, command_as_ctrl: bool) -> Option<Scan> {
    let (code, extended) = match vk {
        0x00 => (0x1e, false), // A
        0x01 => (0x1f, false), // S
        0x02 => (0x20, false), // D
        0x03 => (0x21, false), // F
        0x04 => (0x23, false), // H
        0x05 => (0x22, false), // G
        0x06 => (0x2c, false), // Z
        0x07 => (0x2d, false), // X
        0x08 => (0x2e, false), // C
        0x09 => (0x2f, false), // V
        0x0A => (0x56, false), // ISO section key -> OEM 102
        0x0B => (0x30, false), // B
        0x0C => (0x10, false), // Q
        0x0D => (0x11, false), // W
        0x0E => (0x12, false), // E
        0x0F => (0x13, false), // R
        0x10 => (0x15, false), // Y
        0x11 => (0x14, false), // T
        0x12 => (0x02, false), // 1
        0x13 => (0x03, false), // 2
        0x14 => (0x04, false), // 3
        0x15 => (0x05, false), // 4
        0x16 => (0x07, false), // 6
        0x17 => (0x06, false), // 5
        0x18 => (0x0d, false), // =
        0x19 => (0x0a, false), // 9
        0x1A => (0x08, false), // 7
        0x1B => (0x0c, false), // -
        0x1C => (0x09, false), // 8
        0x1D => (0x0b, false), // 0
        0x1E => (0x1b, false), // ]
        0x1F => (0x18, false), // O
        0x20 => (0x16, false), // U
        0x21 => (0x1a, false), // [
        0x22 => (0x17, false), // I
        0x23 => (0x19, false), // P
        0x24 => (0x1c, false), // Return
        0x25 => (0x26, false), // L
        0x26 => (0x24, false), // J
        0x27 => (0x28, false), // '
        0x28 => (0x25, false), // K
        0x29 => (0x27, false), // ;
        0x2A => (0x2b, false), // backslash
        0x2B => (0x33, false), // ,
        0x2C => (0x35, false), // /
        0x2D => (0x31, false), // N
        0x2E => (0x32, false), // M
        0x2F => (0x34, false), // .
        0x30 => (0x0f, false), // Tab
        0x31 => (0x39, false), // Space
        0x32 => (0x29, false), // `
        0x33 => (0x0e, false), // Delete (backspace)
        0x35 => (0x01, false), // Escape
        0x36 => {
            if command_as_ctrl {
                (0x1d, true)
            } else {
                (0x5c, true)
            }
        } // right Command
        0x37 => {
            if command_as_ctrl {
                (0x1d, false)
            } else {
                (0x5b, true)
            }
        } // left Command
        0x38 => (0x2a, false), // left Shift
        0x39 => (0x3a, false), // Caps Lock
        0x3A => (0x38, false), // left Option -> Alt
        0x3B => {
            if command_as_ctrl {
                (0x5b, true)
            } else {
                (0x1d, false)
            }
        } // left Control
        0x3C => (0x36, false), // right Shift
        0x3D => (0x38, true),  // right Option -> AltGr
        0x3E => {
            if command_as_ctrl {
                (0x5c, true)
            } else {
                (0x1d, true)
            }
        } // right Control
        0x41 => (0x53, false), // keypad .
        0x43 => (0x37, false), // keypad *
        0x45 => (0x4e, false), // keypad +
        0x47 => (0x45, false), // keypad Clear -> Num Lock
        0x4B => (0x35, true),  // keypad /
        0x4C => (0x1c, true),  // keypad Enter
        0x4E => (0x4a, false), // keypad -
        0x51 => (0x59, false), // keypad =
        0x52 => (0x52, false), // keypad 0
        0x53 => (0x4f, false), // keypad 1
        0x54 => (0x50, false), // keypad 2
        0x55 => (0x51, false), // keypad 3
        0x56 => (0x4b, false), // keypad 4
        0x57 => (0x4c, false), // keypad 5
        0x58 => (0x4d, false), // keypad 6
        0x59 => (0x47, false), // keypad 7
        0x5B => (0x48, false), // keypad 8
        0x5C => (0x49, false), // keypad 9
        0x60 => (0x3f, false), // F5
        0x61 => (0x40, false), // F6
        0x62 => (0x41, false), // F7
        0x63 => (0x3d, false), // F3
        0x64 => (0x42, false), // F8
        0x65 => (0x43, false), // F9
        0x67 => (0x57, false), // F11
        0x69 => (0x64, false), // F13
        0x6A => (0x67, false), // F16
        0x6B => (0x65, false), // F14
        0x6D => (0x44, false), // F10
        0x6E => (0x5d, true),  // context menu -> menu
        0x6F => (0x58, false), // F12
        0x71 => (0x66, false), // F15
        0x72 => (0x52, true),  // Help/Insert
        0x73 => (0x47, true),  // Home
        0x74 => (0x49, true),  // Page Up
        0x75 => (0x53, true),  // forward Delete
        0x76 => (0x3e, false), // F4
        0x77 => (0x4f, true),  // End
        0x78 => (0x3c, false), // F2
        0x79 => (0x51, true),  // Page Down
        0x7A => (0x3b, false), // F1
        0x7B => (0x4b, true),  // Left
        0x7C => (0x4d, true),  // Right
        0x7D => (0x50, true),  // Down
        0x7E => (0x48, true),  // Up
        _ => return None,
    };
    Some(Scan { code, extended })
}

/// Map a position across the stream (0..1) to SendInput's absolute
/// coordinates over the whole virtual desktop: the stream shows the display
/// at `origin` (its left or top edge) that is `size` pixels across.
pub(crate) fn absolute(
    value: f32,
    origin: i32,
    screen_size: i32,
    virtual_origin: i32,
    virtual_size: i32,
) -> i32 {
    let value = if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let pixel = f64::from(origin) + f64::from(value) * f64::from((screen_size - 1).max(0));
    ((pixel - f64::from(virtual_origin)) * 65535.0 / f64::from((virtual_size - 1).max(1)))
        .round()
        .clamp(0.0, 65535.0) as i32
}

pub(crate) fn accumulate(delta: f32, remainder: &mut f32) -> i32 {
    if !delta.is_finite() {
        return 0;
    }
    let total = delta + *remainder;
    let whole = total.trunc();
    *remainder = total - whole;
    whole as i32
}

pub(crate) fn scroll(dx: f32, dy: f32, remainder: &mut (f32, f32)) -> (i32, i32) {
    // As on macOS/X11, positive y moves up the page and positive x left.
    // Windows WHEEL positive is up, but HWHEEL positive is right. Keep the
    // Linux scale of 40 pixels per click, retaining sub-120 high-res deltas.
    (
        accumulate(-dx * 3.0, &mut remainder.0),
        accumulate(dy * 3.0, &mut remainder.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_keys_and_extended_pairs() {
        for (mac, code, extended) in [
            (0x00, 0x1e, false),
            (0x24, 0x1c, false),
            (0x4c, 0x1c, true),
            (0x2c, 0x35, false),
            (0x4b, 0x35, true),
            (0x33, 0x0e, false),
            (0x75, 0x53, true),
            (0x41, 0x53, false),
            (0x3d, 0x38, true),
            (0x7b, 0x4b, true),
            (0x7c, 0x4d, true),
            (0x7d, 0x50, true),
            (0x7e, 0x48, true),
            (0x72, 0x52, true),
            (0x73, 0x47, true),
            (0x74, 0x49, true),
            (0x77, 0x4f, true),
            (0x79, 0x51, true),
            (0x0a, 0x56, false),
            (0x69, 0x64, false),
            (0x6a, 0x67, false),
        ] {
            assert_eq!(scan_from_mac(mac, false), Some(Scan { code, extended }));
        }
        assert_eq!(scan_from_mac(0xffff, false), None);
    }

    #[test]
    fn command_control_swap_preserves_sides() {
        for (command, control, win, extended) in
            [(0x37, 0x3b, 0x5b, false), (0x36, 0x3e, 0x5c, true)]
        {
            assert_eq!(
                scan_from_mac(command, false),
                Some(Scan {
                    code: win,
                    extended: true
                })
            );
            assert_eq!(
                scan_from_mac(command, true),
                Some(Scan {
                    code: 0x1d,
                    extended
                })
            );
            assert_eq!(scan_from_mac(control, true), scan_from_mac(command, false));
            assert_eq!(scan_from_mac(control, false), scan_from_mac(command, true));
        }
    }

    #[test]
    fn absolute_primary_and_virtual_desktop() {
        assert_eq!(absolute(0.0, 0, 1920, 0, 1920), 0);
        assert_eq!(absolute(1.0, 0, 1920, 0, 1920), 65535);
        assert_eq!(absolute(0.5, 0, 1920, 0, 1920), 32768);
        assert_eq!(absolute(-2.0, 0, 1920, 0, 1920), 0);
        assert_eq!(absolute(2.0, 0, 1920, 0, 1920), 65535);
        assert_eq!(absolute(0.0, 0, 1920, -1920, 3840), 32776);
        assert_eq!(absolute(1.0, 0, 1920, -1920, 3840), 65535);
        assert_eq!(absolute(f32::NAN, 0, 1, 0, 1), 0);
        // A second display to the right of a 1920-wide primary.
        assert_eq!(absolute(0.0, 1920, 2560, 0, 4480), 28093);
        assert_eq!(absolute(1.0, 1920, 2560, 0, 4480), 65535);
    }

    #[test]
    fn fractional_scroll_and_motion() {
        let mut remainder = (0.0, 0.0);
        assert_eq!(scroll(40.0, 40.0, &mut remainder), (-120, 120));
        assert_eq!(scroll(0.125, -0.125, &mut remainder), (0, 0));
        assert_eq!(scroll(0.125, -0.125, &mut remainder), (0, 0));
        assert_eq!(scroll(0.125, -0.125, &mut remainder), (-1, -1));
        assert_eq!(scroll(f32::NAN, f32::INFINITY, &mut remainder), (0, 0));
        let mut rem = 0.0;
        assert_eq!(accumulate(0.75, &mut rem), 0);
        assert_eq!(accumulate(0.75, &mut rem), 1);
        assert_eq!(accumulate(-1.5, &mut rem), -1);
        assert_eq!(rem, 0.0);
    }
}
