//! macOS input injection via CGEventPost (research/03 §6).
//!
//! Requires the Accessibility permission (System Settings → Privacy & Security
//! → Accessibility) for the process that launched the host; without it, macOS
//! silently discards posted events — we preflight and warn.
//!
//! Keyboard events carry macOS virtual keycodes directly (the viewer maps
//! winit keycodes before sending). A platform-neutral scancode space on the
//! wire is a protocol-v1 task once a second host OS lands.

use std::ffi::c_void;
use std::time::Instant;

use sunna_proto::messages::{GesturePhase, InputEvent, MouseButton};

use crate::InputInjector;

type CGEventRef = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

const TAP_HID: u32 = 0; // kCGHIDEventTap

// CGEventType values.
const EVENT_LEFT_DOWN: u32 = 1;
const EVENT_LEFT_UP: u32 = 2;
const EVENT_RIGHT_DOWN: u32 = 3;
const EVENT_RIGHT_UP: u32 = 4;
const EVENT_MOUSE_MOVED: u32 = 5;
const EVENT_LEFT_DRAGGED: u32 = 6;
const EVENT_RIGHT_DRAGGED: u32 = 7;
const EVENT_OTHER_DOWN: u32 = 25;
const EVENT_OTHER_UP: u32 = 26;
const EVENT_OTHER_DRAGGED: u32 = 27;

const BUTTON_LEFT: u32 = 0;
const BUTTON_RIGHT: u32 = 1;
const BUTTON_CENTER: u32 = 2;

const SCROLL_UNIT_PIXEL: u32 = 0;
const FIELD_CLICK_STATE: u32 = 1; // kCGMouseEventClickState
const FIELD_KEYBOARD_AUTOREPEAT: u32 = 8; // kCGKeyboardEventAutorepeat
const FIELD_SCROLL_IS_CONTINUOUS: u32 = 88; // kCGScrollWheelEventIsContinuous

// CGEventFlags modifier masks.
const FLAG_CAPS_LOCK: u64 = 0x0001_0000;
const CAPS_LOCK: u16 = 0x39;
const FLAG_SHIFT: u64 = 0x0002_0000;
const FLAG_CONTROL: u64 = 0x0004_0000;
const FLAG_OPTION: u64 = 0x0008_0000;
const FLAG_COMMAND: u64 = 0x0010_0000;
const FLAG_FN: u64 = 0x0080_0000;

/// Modifier flag for a mac virtual keycode, if it is a modifier key.
fn modifier_flag(keycode: u16) -> Option<u64> {
    match keycode {
        0x37 | 0x36 => Some(FLAG_COMMAND),
        0x38 | 0x3C => Some(FLAG_SHIFT),
        0x3A | 0x3D => Some(FLAG_OPTION),
        0x3B | 0x3E => Some(FLAG_CONTROL),
        0x39 => Some(FLAG_CAPS_LOCK),
        0x3F => Some(FLAG_FN),
        _ => None,
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGDisplayPixelsWide(display: u32) -> usize;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
    fn CGEventCreate(source: *const c_void) -> CGEventRef;
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn CGEventCreateMouseEvent(
        source: *const c_void,
        mouse_type: u32,
        position: CGPoint,
        button: u32,
    ) -> CGEventRef;
    fn CGEventCreateKeyboardEvent(source: *const c_void, keycode: u16, keydown: bool)
        -> CGEventRef;
    // Not CGEventCreateScrollWheelEvent: that one is variadic after
    // `wheel1`, and Apple arm64 passes variadic arguments on the stack, so a
    // fixed-arity Rust declaration garbles `wheel2` (horizontal scroll).
    fn CGEventCreateScrollWheelEvent2(
        source: *const c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> CGEventRef;
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventPost(tap: u32, event: CGEventRef);
    fn CFRelease(cf: *const c_void);
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
    static kAXTrustedCheckOptionPrompt: *const c_void;
}

pub fn accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Check Accessibility (needed to inject input) and, if missing, have macOS
/// show its prompt pointing at System Settings. Returns whether it's granted
/// now; a grant usually needs the terminal restarted to take effect.
pub fn request_accessibility() -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;
    unsafe {
        if AXIsProcessTrusted() != 0 {
            return true;
        }
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt as _);
        let options = CFDictionary::from_CFType_pairs(&[(
            key.as_CFType(),
            CFBoolean::true_value().as_CFType(),
        )]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as *const c_void) != 0
    }
}

pub struct MacInjector {
    display_size: (f64, f64),
    cursor: CGPoint,
    left_down: bool,
    right_down: bool,
    other_down: bool,
    last_click: Option<(Instant, CGPoint)>,
    click_state: i64,
    /// Sub-pixel scroll carried to the next event (trackpads send fractions).
    scroll_remainder: (f32, f32),
    /// Keys currently held down on the host, released if the session ends.
    keys_down: Vec<u16>,
    /// Caps Lock is a latch, not a held key: viewers send it as a tap per
    /// toggle, so its state is kept here and carried on every event.
    caps_lock: bool,
}

// Only raw CG calls, no shared state.
unsafe impl Send for MacInjector {}

impl MacInjector {
    pub fn new() -> Self {
        if !accessibility_trusted() {
            tracing::warn!(
                "Accessibility permission not granted — injected input will be discarded by \
                 macOS. Enable it in System Settings → Privacy & Security → Accessibility \
                 for the app that launched the host."
            );
        }
        let (width, height, cursor) = unsafe {
            let display = CGMainDisplayID();
            let width = CGDisplayPixelsWide(display) as f64;
            let height = CGDisplayPixelsHigh(display) as f64;
            let probe = CGEventCreate(std::ptr::null());
            let cursor = if probe.is_null() {
                CGPoint {
                    x: width / 2.0,
                    y: height / 2.0,
                }
            } else {
                let location = CGEventGetLocation(probe);
                CFRelease(probe as _);
                location
            };
            (width, height, cursor)
        };
        Self {
            display_size: (width, height),
            cursor,
            left_down: false,
            right_down: false,
            other_down: false,
            last_click: None,
            click_state: 1,
            scroll_remainder: (0.0, 0.0),
            keys_down: Vec::new(),
            caps_lock: false,
        }
    }

    /// Modifier flags for the keys currently held. Posted events don't pick
    /// these up from the system reliably, so every event carries them —
    /// otherwise Cmd+C, Shift-click and friends arrive as plain C and clicks.
    fn flags(&self) -> u64 {
        let caps = if self.caps_lock { FLAG_CAPS_LOCK } else { 0 };
        self.keys_down
            .iter()
            .filter(|&&key| key != CAPS_LOCK)
            .filter_map(|&key| modifier_flag(key))
            .fold(caps, |flags, flag| flags | flag)
    }

    fn post_key(&self, keycode: u16, pressed: bool, repeat: bool) {
        unsafe {
            let event = CGEventCreateKeyboardEvent(std::ptr::null(), keycode, pressed);
            if !event.is_null() {
                CGEventSetFlags(event, self.flags());
                if repeat {
                    CGEventSetIntegerValueField(event, FIELD_KEYBOARD_AUTOREPEAT, 1);
                }
                CGEventPost(TAP_HID, event);
                CFRelease(event as _);
            }
        }
    }

    fn scroll(&mut self, dx: f32, dy: f32, continuous: bool) {
        let x = dx + self.scroll_remainder.0;
        let y = dy + self.scroll_remainder.1;
        let (whole_x, whole_y) = (x.trunc(), y.trunc());
        self.scroll_remainder = (x - whole_x, y - whole_y);
        if whole_x == 0.0 && whole_y == 0.0 {
            return;
        }
        unsafe {
            let event = CGEventCreateScrollWheelEvent2(
                std::ptr::null(),
                SCROLL_UNIT_PIXEL,
                2,
                whole_y as i32,
                whole_x as i32,
                0,
            );
            if !event.is_null() {
                CGEventSetFlags(event, self.flags());
                if continuous {
                    // Trackpad-style scrolling: apps scroll smoothly by pixel
                    // instead of treating each event as a wheel notch.
                    CGEventSetIntegerValueField(event, FIELD_SCROLL_IS_CONTINUOUS, 1);
                }
                CGEventPost(TAP_HID, event);
                CFRelease(event as _);
            }
        }
    }

    fn move_event_type(&self) -> u32 {
        if self.left_down {
            EVENT_LEFT_DRAGGED
        } else if self.right_down {
            EVENT_RIGHT_DRAGGED
        } else if self.other_down {
            EVENT_OTHER_DRAGGED
        } else {
            EVENT_MOUSE_MOVED
        }
    }

    fn drag_button(&self) -> u32 {
        if self.right_down {
            BUTTON_RIGHT
        } else if self.other_down {
            BUTTON_CENTER
        } else {
            BUTTON_LEFT
        }
    }

    fn post_mouse(&self, event_type: u32, button: u32, click_state: Option<i64>) {
        unsafe {
            let event =
                CGEventCreateMouseEvent(std::ptr::null(), event_type, self.cursor, button);
            if event.is_null() {
                return;
            }
            if let Some(clicks) = click_state {
                CGEventSetIntegerValueField(event, FIELD_CLICK_STATE, clicks);
            }
            CGEventSetFlags(event, self.flags());
            CGEventPost(TAP_HID, event);
            CFRelease(event as _);
        }
    }

    fn move_to(&mut self, x: f64, y: f64) {
        self.cursor = CGPoint {
            x: x.clamp(0.0, self.display_size.0 - 1.0),
            y: y.clamp(0.0, self.display_size.1 - 1.0),
        };
        self.post_mouse(self.move_event_type(), self.drag_button(), None);
    }

    fn button(&mut self, button: MouseButton, pressed: bool) {
        let (down_type, up_type, cg_button) = match button {
            MouseButton::Left => (EVENT_LEFT_DOWN, EVENT_LEFT_UP, BUTTON_LEFT),
            MouseButton::Right => (EVENT_RIGHT_DOWN, EVENT_RIGHT_UP, BUTTON_RIGHT),
            MouseButton::Middle => (EVENT_OTHER_DOWN, EVENT_OTHER_UP, BUTTON_CENTER),
            // X buttons map onto "other" with distinct button numbers.
            MouseButton::X1 => (EVENT_OTHER_DOWN, EVENT_OTHER_UP, 3),
            MouseButton::X2 => (EVENT_OTHER_DOWN, EVENT_OTHER_UP, 4),
        };
        match button {
            MouseButton::Left => self.left_down = pressed,
            MouseButton::Right => self.right_down = pressed,
            _ => self.other_down = pressed,
        }
        if pressed {
            // Double-click detection: same spot within the usual interval.
            let now = Instant::now();
            let near_last = self.last_click.map_or(false, |(at, point)| {
                now.duration_since(at).as_millis() < 500
                    && (point.x - self.cursor.x).abs() < 4.0
                    && (point.y - self.cursor.y).abs() < 4.0
            });
            self.click_state = if near_last { self.click_state + 1 } else { 1 };
            self.last_click = Some((now, self.cursor));
        }
        let event_type = if pressed { down_type } else { up_type };
        self.post_mouse(event_type, cg_button, Some(self.click_state));
    }
}

impl Default for MacInjector {
    fn default() -> Self {
        Self::new()
    }
}

impl InputInjector for MacInjector {
    fn inject(&mut self, event: &InputEvent) -> anyhow::Result<()> {
        match *event {
            InputEvent::MouseMoveAbs { x, y } => {
                self.move_to(
                    x as f64 * self.display_size.0,
                    y as f64 * self.display_size.1,
                );
            }
            InputEvent::MouseMoveRel { dx, dy } => {
                self.move_to(self.cursor.x + dx as f64, self.cursor.y + dy as f64);
            }
            InputEvent::MouseButton { button, pressed } => self.button(button, pressed),
            InputEvent::Scroll { dx, dy, phase, .. } => self.scroll(dx, dy, phase.is_some()),
            InputEvent::Key {
                scancode,
                pressed,
                repeat,
            } => {
                if scancode == CAPS_LOCK {
                    if pressed && !repeat {
                        self.caps_lock = !self.caps_lock;
                    }
                } else if pressed {
                    if !self.keys_down.contains(&scancode) {
                        self.keys_down.push(scancode);
                    }
                } else {
                    self.keys_down.retain(|&key| key != scancode);
                }
                self.post_key(scancode, pressed, repeat);
            }
            InputEvent::Gesture { kind, phase, .. } => {
                // Semantic gesture replay (research/05 §4) is a later milestone.
                if phase == GesturePhase::Begin {
                    tracing::debug!(?kind, "gesture replay not implemented yet");
                }
            }
        }
        Ok(())
    }

    fn release_all(&mut self) {
        // Release non-modifiers first, while their modifiers still apply.
        let mut keys = self.keys_down.clone();
        keys.sort_by_key(|&key| modifier_flag(key).is_some());
        for keycode in keys {
            self.keys_down.retain(|&key| key != keycode);
            self.post_key(keycode, false, false);
        }
        if self.left_down {
            self.button(MouseButton::Left, false);
        }
        if self.right_down {
            self.button(MouseButton::Right, false);
        }
        if self.other_down {
            self.button(MouseButton::Middle, false);
        }
        self.scroll_remainder = (0.0, 0.0);
    }
}
