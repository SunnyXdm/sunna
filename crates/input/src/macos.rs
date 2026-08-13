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
    fn CGEventCreateScrollWheelEvent(
        source: *const c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
    ) -> CGEventRef;
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    fn CGEventPost(tap: u32, event: CGEventRef);
    fn CFRelease(cf: *const c_void);
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

pub fn accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

pub struct MacInjector {
    display_size: (f64, f64),
    cursor: CGPoint,
    left_down: bool,
    right_down: bool,
    other_down: bool,
    last_click: Option<(Instant, CGPoint)>,
    click_state: i64,
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
            InputEvent::Scroll { dx, dy, .. } => unsafe {
                let event = CGEventCreateScrollWheelEvent(
                    std::ptr::null(),
                    SCROLL_UNIT_PIXEL,
                    2,
                    dy as i32,
                    dx as i32,
                );
                if !event.is_null() {
                    CGEventPost(TAP_HID, event);
                    CFRelease(event as _);
                }
            },
            InputEvent::Key { scancode, pressed } => unsafe {
                let event = CGEventCreateKeyboardEvent(std::ptr::null(), scancode, pressed);
                if !event.is_null() {
                    CGEventPost(TAP_HID, event);
                    CFRelease(event as _);
                }
            },
            InputEvent::Gesture { kind, phase, .. } => {
                // Semantic gesture replay (research/05 §4) is a later milestone.
                if phase == GesturePhase::Begin {
                    tracing::debug!(?kind, "gesture replay not implemented yet");
                }
            }
        }
        Ok(())
    }
}
