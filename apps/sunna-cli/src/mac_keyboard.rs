//! Keyboard capture for the macOS viewer: a HID-level event tap (where
//! QEMU's full grab, and so Try Omarchy, takes keys) so ⌘Tab, ⌘Space, ⌘Q,
//! Ctrl+arrows and the rest reach the host instead of macOS while the viewer
//! window is focused. Needs the Accessibility permission for the app that
//! launched the viewer; without it the window's ordinary key events are
//! used, and macOS keeps its shortcuts.
//!
//! Captured keys never reach the window, so the two paths don't double up.
//! The tap runs on the main run loop, the same thread as the viewer.

use std::ffi::c_void;
use std::sync::{Arc, Mutex};

use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};
use sunna_proto::messages::InputEvent;
use tokio::sync::mpsc::UnboundedSender;
use winit::event_loop::EventLoopProxy;

use crate::keyboard::{Action, Hotkey, RawKind, Translator};
use crate::viewer::{FrameReady, SharedFrame};

const HID_EVENT_TAP: u32 = 0; // kCGHIDEventTap
const HEAD_INSERT_EVENT_TAP: u32 = 0; // kCGHeadInsertEventTap
const TAP_OPTION_DEFAULT: u32 = 0; // active: may swallow events
const EVENT_KEY_DOWN: u32 = 10;
const EVENT_KEY_UP: u32 = 11;
const EVENT_FLAGS_CHANGED: u32 = 12;
const EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
const FIELD_AUTOREPEAT: u32 = 8; // kCGKeyboardEventAutorepeat
const FIELD_KEYCODE: u32 = 9; // kCGKeyboardEventKeycode

type TapCallback = unsafe extern "C" fn(
    proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    user_info: *mut c_void,
) -> *mut c_void;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: TapCallback,
        user_info: *mut c_void,
    ) -> *mut c_void;
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
    fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
    fn CGEventGetFlags(event: *mut c_void) -> u64;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopCommonModes: *const c_void;
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: *mut c_void,
        order: isize,
    ) -> *mut c_void;
    fn CFMachPortInvalidate(port: *mut c_void);
    fn CFRunLoopGetMain() -> *mut c_void;
    fn CFRunLoopAddSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopRemoveSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRelease(cf: *const c_void);
}

struct TapState {
    translator: Translator,
    /// The viewer window is key.
    focused: bool,
    /// Capture wanted (⌃⌥G toggles it).
    enabled: bool,
    tap: *mut c_void,
    input: UnboundedSender<InputEvent>,
    shared: Arc<SharedFrame>,
    wake: EventLoopProxy<FrameReady>,
}

impl TapState {
    fn apply(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Send(event) => {
                    let _ = self.input.send(event);
                }
                Action::Hotkey(hotkey) => {
                    if hotkey == Hotkey::ToggleCapture {
                        // Straight away, so the rest of this chord already
                        // goes to macOS.
                        self.enabled = false;
                        let released = self.translator.release_all();
                        self.apply(released);
                    }
                    self.shared.hotkeys.lock().unwrap().push(hotkey);
                    let _ = self.wake.send_event(FrameReady);
                }
            }
        }
    }
}

pub struct KeyboardCapture {
    state: Box<Mutex<TapState>>,
    tap: *mut c_void,
    source: *mut c_void,
}

impl KeyboardCapture {
    /// Install the tap. Fails without the Accessibility permission (macOS
    /// is asked to prompt for it).
    pub fn new(
        input: UnboundedSender<InputEvent>,
        shared: Arc<SharedFrame>,
        wake: EventLoopProxy<FrameReady>,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            sunna_input::macos::request_accessibility(),
            "Accessibility permission not granted"
        );
        let state = Box::new(Mutex::new(TapState {
            translator: Translator::default(),
            focused: true,
            enabled: true,
            tap: std::ptr::null_mut(),
            input,
            shared,
            wake,
        }));
        let mask =
            (1u64 << EVENT_KEY_DOWN) | (1u64 << EVENT_KEY_UP) | (1u64 << EVENT_FLAGS_CHANGED);
        // SAFETY: `state` is boxed, so the pointer handed to the tap stays
        // valid until Drop, which tears the tap down before freeing it.
        let tap = unsafe {
            CGEventTapCreate(
                HID_EVENT_TAP,
                HEAD_INSERT_EVENT_TAP,
                TAP_OPTION_DEFAULT,
                mask,
                tap_callback,
                &*state as *const Mutex<TapState> as *mut c_void,
            )
        };
        anyhow::ensure!(!tap.is_null(), "CGEventTapCreate failed");
        state.lock().unwrap().tap = tap;
        // SAFETY: `tap` is a live CFMachPort; the source is released in Drop.
        let source = unsafe {
            let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            CFRunLoopAddSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
            source
        };
        tracing::info!("keyboard capture on: system shortcuts go to the host while focused");
        Ok(Self { state, tap, source })
    }

    /// Whether ⌘Tab and friends currently go to the host (when focused).
    pub fn enabled(&self) -> bool {
        self.state.lock().unwrap().enabled
    }

    pub fn set_enabled(&self, enabled: bool) {
        let mut state = self.state.lock().unwrap();
        if state.enabled && !enabled {
            let released = state.translator.release_all();
            state.apply(released);
        }
        state.enabled = enabled;
    }

    /// Track window focus: keys only go to the host while focused, and
    /// everything held is released when focus leaves (its key-ups will go
    /// to another app).
    pub fn set_focused(&self, focused: bool) {
        let mut state = self.state.lock().unwrap();
        state.focused = focused;
        if !focused {
            let released = state.translator.release_all();
            state.apply(released);
            state.translator.forget_hotkeys();
        }
    }
}

impl Drop for KeyboardCapture {
    fn drop(&mut self) {
        // SAFETY: undo `new` in reverse; after this the callback can't run,
        // so `state` may be freed.
        unsafe {
            CGEventTapEnable(self.tap, false);
            CFRunLoopRemoveSource(CFRunLoopGetMain(), self.source, kCFRunLoopCommonModes);
            CFMachPortInvalidate(self.tap);
            CFRelease(self.source);
            CFRelease(self.tap);
        }
        let released = self.state.lock().unwrap().translator.release_all();
        self.state.lock().unwrap().apply(released);
    }
}

/// The viewer is the active app and has a key window. Main thread only
/// (the tap runs on the main run loop).
fn viewer_is_key() -> bool {
    // SAFETY: plain AppKit getters on the main thread.
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return false;
        }
        let active: Bool = msg_send![app, isActive];
        let key_window: *mut AnyObject = msg_send![app, keyWindow];
        active.as_bool() && !key_window.is_null()
    }
}

unsafe extern "C" fn tap_callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    user_info: *mut c_void,
) -> *mut c_void {
    // SAFETY: `user_info` is the boxed state, alive while the tap exists.
    let state = unsafe { &*(user_info as *const Mutex<TapState>) };
    // Main thread only, so this never contends; if it somehow did, let the
    // event through rather than block the window server.
    let Ok(mut state) = state.try_lock() else {
        return event;
    };
    if event_type == EVENT_TAP_DISABLED_BY_TIMEOUT || event_type == EVENT_TAP_DISABLED_BY_USER_INPUT
    {
        // macOS switches a tap off if it's slow, and never back on by
        // itself: without this, ⌘Space would quietly start opening
        // Spotlight mid-session (Try Omarchy patches QEMU for the same).
        // SAFETY: `tap` was set right after creation and outlives us.
        unsafe { CGEventTapEnable(state.tap, true) };
        tracing::warn!(event_type, "macOS disabled the keyboard tap; re-enabled");
        return event;
    }
    let kind = match event_type {
        EVENT_KEY_DOWN => RawKind::Down,
        EVENT_KEY_UP => RawKind::Up,
        EVENT_FLAGS_CHANGED => RawKind::FlagsChanged,
        _ => return event,
    };
    // Also ask AppKit: if a focus change were ever missed, this tap would
    // otherwise swallow every key on the Mac.
    if !state.focused || !viewer_is_key() {
        return event;
    }
    // SAFETY: `event` is the live CGEvent macOS handed us.
    let (keycode, flags, repeat) = unsafe {
        (
            CGEventGetIntegerValueField(event, FIELD_KEYCODE) as u16,
            CGEventGetFlags(event),
            CGEventGetIntegerValueField(event, FIELD_AUTOREPEAT) != 0,
        )
    };
    if !state.enabled {
        // Released: macOS keeps its shortcuts and the window's own key
        // events reach the host, except the rest of the ⌃⌥G that released
        // us (its repeats and key-up).
        if kind != RawKind::FlagsChanged && state.translator.owns(keycode) {
            if kind == RawKind::Up {
                state.translator.take_hotkey_up(keycode);
            }
            return std::ptr::null_mut();
        }
        return event;
    }
    let actions = state.translator.handle(kind, keycode, flags, repeat);
    state.apply(actions);
    std::ptr::null_mut()
}
