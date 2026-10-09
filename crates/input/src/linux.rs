//! Linux input injection through the X11 XTEST extension (Xorg and Xvfb).
//! `uinput` (research/03 §6) replaces this for Wayland and headless DRM.
//!
//! The wire carries macOS virtual keycodes (the only viewer is a Mac so
//! far); they're mapped to evdev codes, which X keycodes are offset from.

use std::collections::HashSet;

use anyhow::Context;
use sunna_proto::messages::{GestureKind, GesturePhase, InputEvent, MouseButton};

/// Pinch to zoom, as Linux apps do it: Ctrl + a wheel click per step of
/// this much magnification (log2; about 9%).
const PINCH_STEP: f32 = 0.125;
/// Left Control (evdev 29), as an X keycode.
const X_LEFT_CONTROL: u8 = 29 + 8;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConnectionExt as _, Window, BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, KEY_PRESS_EVENT,
    KEY_RELEASE_EVENT, MOTION_NOTIFY_EVENT,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

use crate::InputInjector;

/// Pixels of scroll per wheel click sent to X (which only knows clicks).
const SCROLL_PIXELS_PER_CLICK: f32 = 40.0;

pub struct X11Injector {
    conn: RustConnection,
    root: Window,
    size: (f32, f32),
    cursor: (i16, i16),
    keys_down: HashSet<u8>,
    buttons_down: HashSet<u8>,
    scroll_remainder: (f32, f32),
    /// Pinch magnification not yet turned into a zoom step (log2).
    pinch: f32,
    /// Send the Mac's Command key as Control, so Cmd+C/V/Z do what a Mac
    /// user expects (`SUNNA_MAC_COMMAND=ctrl`). Default: Super.
    command_as_ctrl: bool,
}

impl X11Injector {
    pub fn new() -> anyhow::Result<Self> {
        let (conn, screen_num) =
            x11rb::connect(None).context("connecting to the X server (is DISPLAY set?)")?;
        conn.xtest_get_version(2, 2)?
            .reply()
            .context("the X server has no XTEST extension")?;
        let screen = &conn.setup().roots[screen_num];
        let (root, size) = (
            screen.root,
            (
                screen.width_in_pixels as f32,
                screen.height_in_pixels as f32,
            ),
        );
        let command_as_ctrl = std::env::var("SUNNA_MAC_COMMAND").is_ok_and(|value| value == "ctrl");
        tracing::info!(command_as_ctrl, "X11 input injection (XTEST)");
        Ok(Self {
            conn,
            root,
            size,
            cursor: (0, 0),
            keys_down: HashSet::new(),
            buttons_down: HashSet::new(),
            scroll_remainder: (0.0, 0.0),
            pinch: 0.0,
            command_as_ctrl,
        })
    }

    fn fake(&self, kind: u8, detail: u8, x: i16, y: i16) -> anyhow::Result<()> {
        self.conn
            .xtest_fake_input(kind, detail, 0, self.root, x, y, 0)?;
        Ok(())
    }

    fn key(&mut self, keycode: u8, pressed: bool) -> anyhow::Result<()> {
        if pressed {
            self.keys_down.insert(keycode);
        } else if !self.keys_down.remove(&keycode) {
            return Ok(());
        }
        let kind = if pressed {
            KEY_PRESS_EVENT
        } else {
            KEY_RELEASE_EVENT
        };
        self.fake(kind, keycode, 0, 0)
    }

    fn button(&mut self, button: u8, pressed: bool) -> anyhow::Result<()> {
        if pressed {
            self.buttons_down.insert(button);
        } else if !self.buttons_down.remove(&button) {
            return Ok(());
        }
        let kind = if pressed {
            BUTTON_PRESS_EVENT
        } else {
            BUTTON_RELEASE_EVENT
        };
        self.fake(kind, button, 0, 0)
    }

    fn click(&self, button: u8, times: u32) -> anyhow::Result<()> {
        for _ in 0..times {
            self.fake(BUTTON_PRESS_EVENT, button, 0, 0)?;
            self.fake(BUTTON_RELEASE_EVENT, button, 0, 0)?;
        }
        Ok(())
    }

    fn move_to(&mut self, x: f32, y: f32) -> anyhow::Result<()> {
        let x = x.clamp(0.0, self.size.0 - 1.0) as i16;
        let y = y.clamp(0.0, self.size.1 - 1.0) as i16;
        self.cursor = (x, y);
        // detail 0 = absolute motion.
        self.fake(MOTION_NOTIFY_EVENT, 0, x, y)
    }

    fn scroll(&mut self, dx: f32, dy: f32) -> anyhow::Result<()> {
        let x = dx / SCROLL_PIXELS_PER_CLICK + self.scroll_remainder.0;
        let y = dy / SCROLL_PIXELS_PER_CLICK + self.scroll_remainder.1;
        let (whole_x, whole_y) = (x.trunc(), y.trunc());
        self.scroll_remainder = (x - whole_x, y - whole_y);
        // macOS deltas: positive y scrolls content down (up the page); X
        // buttons 4/5 are up/down and 6/7 left/right.
        if whole_y != 0.0 {
            self.click(if whole_y > 0.0 { 4 } else { 5 }, whole_y.abs() as u32)?;
        }
        if whole_x != 0.0 {
            self.click(if whole_x > 0.0 { 6 } else { 7 }, whole_x.abs() as u32)?;
        }
        Ok(())
    }
}

impl InputInjector for X11Injector {
    fn inject(&mut self, event: &InputEvent) -> anyhow::Result<()> {
        match *event {
            InputEvent::MouseMoveAbs { x, y } => self.move_to(x * self.size.0, y * self.size.1)?,
            InputEvent::MouseMoveRel { dx, dy } => {
                self.move_to(self.cursor.0 as f32 + dx, self.cursor.1 as f32 + dy)?
            }
            InputEvent::MouseButton { button, pressed } => {
                let button = match button {
                    MouseButton::Left => 1,
                    MouseButton::Middle => 2,
                    MouseButton::Right => 3,
                    MouseButton::X1 => 8,
                    MouseButton::X2 => 9,
                };
                self.button(button, pressed)?;
            }
            InputEvent::Scroll { dx, dy, phase, .. } => {
                if matches!(phase, Some(GesturePhase::Begin) | Some(GesturePhase::End)) {
                    self.scroll_remainder = (0.0, 0.0);
                }
                self.scroll(dx, dy)?;
            }
            InputEvent::Key {
                scancode,
                pressed,
                repeat,
            } => {
                let Some(code) = evdev_from_mac(scancode, self.command_as_ctrl) else {
                    tracing::debug!(scancode, "no Linux key for this Mac keycode");
                    return Ok(());
                };
                // The X server auto-repeats held keys itself, XTEST ones
                // included; replaying the Mac's repeats too doubled the rate.
                if repeat {
                    return Ok(());
                }
                self.key((code + 8) as u8, pressed)?;
            }
            InputEvent::Gesture { kind: GestureKind::Pinch, phase, scale_delta, .. } => {
                if phase == GesturePhase::Begin {
                    self.pinch = 0.0;
                }
                self.pinch += scale_delta;
                let steps = (self.pinch / PINCH_STEP).trunc();
                if steps != 0.0 {
                    self.pinch -= steps * PINCH_STEP;
                    // Ctrl for the clicks only, unless it's already held.
                    let held = self.keys_down.contains(&X_LEFT_CONTROL);
                    if !held {
                        self.fake(KEY_PRESS_EVENT, X_LEFT_CONTROL, 0, 0)?;
                    }
                    // Wheel up (4) zooms in, down (5) out.
                    self.click(if steps > 0.0 { 4 } else { 5 }, steps.abs() as u32)?;
                    if !held {
                        self.fake(KEY_RELEASE_EVENT, X_LEFT_CONTROL, 0, 0)?;
                    }
                }
            }
            InputEvent::Gesture { .. } => {}
        }
        self.conn.flush()?;
        Ok(())
    }

    fn release_all(&mut self) {
        for keycode in std::mem::take(&mut self.keys_down) {
            let _ = self.fake(KEY_RELEASE_EVENT, keycode, 0, 0);
        }
        for button in std::mem::take(&mut self.buttons_down) {
            let _ = self.fake(BUTTON_RELEASE_EVENT, button, 0, 0);
        }
        self.scroll_remainder = (0.0, 0.0);
        // Wait for the server to take the releases: the connection may be
        // dropped right after this.
        let _ = self.conn.get_input_focus().map(|cookie| cookie.reply());
    }
}

/// macOS virtual keycode (Carbon `kVK_*`) → Linux evdev `KEY_*` code.
pub fn evdev_from_mac(vk: u16, command_as_ctrl: bool) -> Option<u16> {
    Some(match vk {
        0x00 => 30, // A
        0x01 => 31, // S
        0x02 => 32, // D
        0x03 => 33, // F
        0x04 => 35, // H
        0x05 => 34, // G
        0x06 => 44, // Z
        0x07 => 45, // X
        0x08 => 46, // C
        0x09 => 47, // V
        0x0A => 86, // ISO section key -> KEY_102ND
        0x0B => 48, // B
        0x0C => 16, // Q
        0x0D => 17, // W
        0x0E => 18, // E
        0x0F => 19, // R
        0x10 => 21, // Y
        0x11 => 20, // T
        0x12 => 2,  // 1
        0x13 => 3,  // 2
        0x14 => 4,  // 3
        0x15 => 5,  // 4
        0x16 => 7,  // 6
        0x17 => 6,  // 5
        0x18 => 13, // =
        0x19 => 10, // 9
        0x1A => 8,  // 7
        0x1B => 12, // -
        0x1C => 9,  // 8
        0x1D => 11, // 0
        0x1E => 27, // ]
        0x1F => 24, // O
        0x20 => 22, // U
        0x21 => 26, // [
        0x22 => 23, // I
        0x23 => 25, // P
        0x24 => 28, // Return
        0x25 => 38, // L
        0x26 => 36, // J
        0x27 => 40, // '
        0x28 => 37, // K
        0x29 => 39, // ;
        0x2A => 43, // backslash
        0x2B => 51, // ,
        0x2C => 53, // /
        0x2D => 49, // N
        0x2E => 50, // M
        0x2F => 52, // .
        0x30 => 15, // Tab
        0x31 => 57, // Space
        0x32 => 41, // `
        0x33 => 14, // Delete (backspace)
        0x35 => 1,  // Escape
        0x36 => {
            if command_as_ctrl {
                97
            } else {
                126
            }
        } // right Command
        0x37 => {
            if command_as_ctrl {
                29
            } else {
                125
            }
        } // left Command
        0x38 => 42, // left Shift
        0x39 => 58, // Caps Lock
        0x3A => 56, // left Option -> Alt
        0x3B => {
            if command_as_ctrl {
                125
            } else {
                29
            }
        } // left Control
        0x3C => 54, // right Shift
        0x3D => 100, // right Option -> AltGr
        0x3E => {
            if command_as_ctrl {
                126
            } else {
                97
            }
        } // right Control
        0x41 => 83, // keypad .
        0x43 => 55, // keypad *
        0x45 => 78, // keypad +
        0x47 => 69, // keypad Clear -> Num Lock
        0x4B => 98, // keypad /
        0x4C => 96, // keypad Enter
        0x4E => 74, // keypad -
        0x51 => 117, // keypad =
        0x52 => 82, // keypad 0
        0x53 => 79, // keypad 1
        0x54 => 80, // keypad 2
        0x55 => 81, // keypad 3
        0x56 => 75, // keypad 4
        0x57 => 76, // keypad 5
        0x58 => 77, // keypad 6
        0x59 => 71, // keypad 7
        0x5B => 72, // keypad 8
        0x5C => 73, // keypad 9
        0x60 => 63, // F5
        0x61 => 64, // F6
        0x62 => 65, // F7
        0x63 => 61, // F3
        0x64 => 66, // F8
        0x65 => 67, // F9
        0x67 => 87, // F11
        0x69 => 183, // F13
        0x6A => 186, // F16
        0x6B => 184, // F14
        0x6D => 68, // F10
        0x6E => 127, // context menu -> KEY_COMPOSE
        0x6F => 88, // F12
        0x71 => 185, // F15
        0x72 => 110, // Help/Insert
        0x73 => 102, // Home
        0x74 => 104, // Page Up
        0x75 => 111, // forward Delete
        0x76 => 62, // F4
        0x77 => 107, // End
        0x78 => 60, // F2
        0x79 => 109, // Page Down
        0x7A => 59, // F1
        0x7B => 105, // Left
        0x7C => 106, // Right
        0x7D => 108, // Down
        0x7E => 103, // Up
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::evdev_from_mac;

    #[test]
    fn command_mapping() {
        assert_eq!(evdev_from_mac(0x37, false), Some(125)); // Super
        assert_eq!(evdev_from_mac(0x37, true), Some(29)); // Ctrl
        assert_eq!(evdev_from_mac(0x3B, true), Some(125)); // swapped
        assert_eq!(evdev_from_mac(0x00, false), Some(30)); // A
    }

    /// Needs an X server: `DISPLAY=:40 cargo test -p sunna-input -- --ignored`.
    /// Moves the pointer to the centre, then types "hi" + Return into
    /// whatever has focus.
    #[test]
    #[ignore]
    fn injects_into_x() {
        use super::*;
        let mut injector = X11Injector::new().unwrap();
        injector
            .inject(&InputEvent::MouseMoveAbs { x: 0.5, y: 0.5 })
            .unwrap();
        let pointer = injector
            .conn
            .query_pointer(injector.root)
            .unwrap()
            .reply()
            .unwrap();
        assert_eq!(
            (pointer.root_x, pointer.root_y),
            (
                (injector.size.0 / 2.0) as i16,
                (injector.size.1 / 2.0) as i16
            )
        );
        for scancode in [0x04u16, 0x22, 0x24] {
            for pressed in [true, false] {
                injector
                    .inject(&InputEvent::Key {
                        scancode,
                        pressed,
                        repeat: false,
                    })
                    .unwrap();
            }
        }
        // Round trip so the server has handled everything before we exit.
        injector.conn.get_input_focus().unwrap().reply().unwrap();
    }

    /// Needs an X server (a throwaway one is best: `Xvfb :49 &`, then
    /// `DISPLAY=:49 cargo test -p sunna-input -- --ignored pinch`).
    #[test]
    #[ignore]
    fn a_pinch_is_ctrl_and_the_wheel() {
        use super::*;
        use sunna_proto::messages::GestureKind;
        use x11rb::protocol::xproto::{CreateWindowAux, EventMask, InputFocus, WindowClass};
        use x11rb::protocol::Event;
        use x11rb::COPY_DEPTH_FROM_PARENT;

        // A window of our own, focused and under the pointer, that records
        // what arrives.
        let (conn, screen) = x11rb::connect(None).unwrap();
        let root = conn.setup().roots[screen].root;
        let window = conn.generate_id().unwrap();
        let mask = EventMask::KEY_PRESS | EventMask::KEY_RELEASE | EventMask::BUTTON_PRESS;
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            400,
            400,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().override_redirect(1).event_mask(mask),
        )
        .unwrap();
        conn.map_window(window).unwrap();
        conn.set_input_focus(InputFocus::POINTER_ROOT, window, 0u32).unwrap();
        conn.get_input_focus().unwrap().reply().unwrap();

        let mut injector = X11Injector::new().unwrap();
        injector.inject(&InputEvent::MouseMoveAbs { x: 0.01, y: 0.01 }).unwrap();
        let pinch = |phase, scale_delta| InputEvent::Gesture {
            kind: GestureKind::Pinch,
            phase,
            fingers: 2,
            dx: 0.0,
            dy: 0.0,
            velocity_x: 0.0,
            velocity_y: 0.0,
            scale_delta,
            rotation_delta: 0.0,
        };
        // Two steps' worth of zooming in, in small pieces.
        injector.inject(&pinch(GesturePhase::Begin, 0.0)).unwrap();
        for _ in 0..5 {
            injector.inject(&pinch(GesturePhase::Update, 0.06)).unwrap();
        }
        injector.inject(&pinch(GesturePhase::End, 0.0)).unwrap();
        injector.conn.get_input_focus().unwrap().reply().unwrap();
        conn.get_input_focus().unwrap().reply().unwrap();

        let mut seen = Vec::new();
        while let Some(event) = conn.poll_for_event().unwrap() {
            match event {
                Event::KeyPress(key) => seen.push(format!("press {}", key.detail)),
                Event::KeyRelease(key) => seen.push(format!("release {}", key.detail)),
                Event::ButtonPress(button) => seen.push(format!("wheel {} ctrl={}", button.detail, u16::from(button.state) & 4 != 0)),
                _ => {}
            }
        }
        // A step as each piece completes one, Ctrl held for its click.
        assert_eq!(
            seen,
            ["press 37", "wheel 4 ctrl=true", "release 37", "press 37", "wheel 4 ctrl=true", "release 37"]
        );
    }
}
