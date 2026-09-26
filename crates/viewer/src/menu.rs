//! The in-session menu (macOS): a native dark `NSMenu` popped up from the
//! floating button or ⌃⌥M, Parsec-style.
//!
//! It's popped from a block on the main dispatch queue, not from inside a
//! winit callback: winit skips its event handling while a handler is
//! running, so a menu tracked inside one would freeze the video until it
//! closed. From the run loop, frames keep arriving under the open menu.
//!
//! Choices come back as [`MenuEvent`]s through a queue the viewer drains
//! when woken, followed by `Closed` once the menu is gone.

use std::ffi::{c_void, CString};
use std::sync::{Mutex, OnceLock};

use objc2::encode::{Encode, Encoding};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, Bool, NSObject, Sel};
use objc2::{class, define_class, msg_send, sel, ClassType};
use winit::event_loop::EventLoopProxy;

use crate::viewer::FrameReady;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    ToggleStats,
    ToggleFullscreen,
    ToggleCapture,
    HideButton,
    Disconnect,
    Codec(Codec),
    /// Stream size as a percentage of this screen's.
    Scale(u8),
    BitrateMbps(u32),
    ToggleFastLane,
    /// One of the host's shortcuts (`send_keys::shortcuts_for`), by index.
    SendShortcut(u8),
    ToggleClipboard,
    TypeClipboard,
    FrameRate(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Hevc,
    H264,
}

impl Codec {
    pub fn name(self) -> &'static str {
        match self {
            Codec::Hevc => "hevc",
            Codec::H264 => "h264",
        }
    }
}

pub const SCALES: [u8; 3] = [100, 75, 50];
pub const BITRATES_MBPS: [u32; 4] = [10, 20, 40, 80];
pub const FRAME_RATES: [u32; 2] = [60, 30];

impl MenuAction {
    /// NSMenuItem tags: small integers, parameters folded in.
    fn tag(self) -> isize {
        match self {
            MenuAction::ToggleStats => 1,
            MenuAction::ToggleFullscreen => 2,
            MenuAction::ToggleCapture => 3,
            MenuAction::HideButton => 4,
            MenuAction::Disconnect => 5,
            MenuAction::Codec(Codec::Hevc) => 10,
            MenuAction::Codec(Codec::H264) => 11,
            MenuAction::Scale(percent) => 1000 + percent as isize,
            MenuAction::BitrateMbps(mbps) => 2000 + mbps as isize,
            MenuAction::ToggleFastLane => 6,
            MenuAction::ToggleClipboard => 7,
            MenuAction::TypeClipboard => 8,
            MenuAction::SendShortcut(index) => 3000 + index as isize,
            MenuAction::FrameRate(fps) => 4000 + fps as isize,
        }
    }

    fn from_tag(tag: isize) -> Option<Self> {
        Some(match tag {
            1 => MenuAction::ToggleStats,
            2 => MenuAction::ToggleFullscreen,
            3 => MenuAction::ToggleCapture,
            4 => MenuAction::HideButton,
            5 => MenuAction::Disconnect,
            6 => MenuAction::ToggleFastLane,
            7 => MenuAction::ToggleClipboard,
            8 => MenuAction::TypeClipboard,
            10 => MenuAction::Codec(Codec::Hevc),
            11 => MenuAction::Codec(Codec::H264),
            1000..=1100 => MenuAction::Scale((tag - 1000) as u8),
            2000..=2999 => MenuAction::BitrateMbps((tag - 2000) as u32),
            3000..=3099 => MenuAction::SendShortcut((tag - 3000) as u8),
            4000..=4240 => MenuAction::FrameRate((tag - 4000) as u32),
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuEvent {
    Chose(MenuAction),
    Closed,
}

/// What the menu shows checked or enabled.
#[derive(Clone)]
pub struct MenuState {
    /// Heading lines: the host, then what's streaming.
    pub host: String,
    pub detail: String,
    /// Shortcuts that suit the host's OS.
    pub shortcuts: &'static [crate::send_keys::Shortcut],
    /// Clipboard sharing is on.
    pub clipboard: bool,
    /// The stream's frame rate.
    pub fps: u32,
    pub stats: bool,
    pub fullscreen: bool,
    /// ⌘ shortcuts currently go to the remote.
    pub capture: bool,
    /// Keyboard capture is possible at all (Accessibility granted).
    pub capture_available: bool,
    pub button_visible: bool,
    /// The stream now: codec name, size, fast lane.
    pub codec: &'static str,
    pub stream_size: (u32, u32),
    pub fast_lane: bool,
    /// What the viewer asked for (None: the host's default).
    pub scale: u8,
    pub bitrate_mbps: Option<u32>,
    /// Stream settings can change live (the host supports it).
    pub video_available: bool,
}

static EVENTS: Mutex<Vec<MenuEvent>> = Mutex::new(Vec::new());
static WAKE: OnceLock<Mutex<EventLoopProxy<FrameReady>>> = OnceLock::new();

fn push(event: MenuEvent) {
    EVENTS.lock().unwrap().push(event);
    if let Some(wake) = WAKE.get() {
        let _ = wake.lock().unwrap().send_event(FrameReady);
    }
}

/// Menu choices and closings since the last call.
pub fn take_events() -> Vec<MenuEvent> {
    std::mem::take(&mut *EVENTS.lock().unwrap())
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; no Drop impl.
    #[unsafe(super(NSObject))]
    #[name = "SunnaMenuTarget"]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(menuAction:))]
        fn menu_action(&self, sender: *mut AnyObject) {
            // SAFETY: AppKit passes the chosen NSMenuItem.
            let tag: isize = unsafe { msg_send![sender, tag] };
            if let Some(action) = MenuAction::from_tag(tag) {
                push(MenuEvent::Chose(action));
            }
        }
    }
);

/// Everything the block needs to build and show the menu.
struct PopUp {
    view: *mut AnyObject,
    location: (f64, f64),
    state: MenuState,
}

#[link(name = "System", kind = "dylib")]
extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_async_f(
        queue: *const c_void,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
}

/// Show the menu at `location` (points, in `view`'s coordinates) from the
/// main run loop. Returns at once; see the module docs.
pub fn pop_up(
    proxy: &EventLoopProxy<FrameReady>,
    view: *mut AnyObject,
    location: (f64, f64),
    state: MenuState,
) {
    let _ = WAKE.set(Mutex::new(proxy.clone()));
    let job = Box::into_raw(Box::new(PopUp {
        view,
        location,
        state,
    }));
    // SAFETY: the main queue outlives us; `run_pop_up` takes the box back.
    unsafe {
        dispatch_async_f(&_dispatch_main_q, job.cast(), run_pop_up);
    }
}

extern "C" fn run_pop_up(context: *mut c_void) {
    // SAFETY: made by `pop_up` from a Box<PopUp>, used once.
    let job = unsafe { Box::from_raw(context.cast::<PopUp>()) };
    // SAFETY: AppKit calls on the main thread (the main dispatch queue);
    // the view belongs to the viewer window, which outlives the session.
    unsafe { show(&job) };
    push(MenuEvent::Closed);
}

const CONTROL: usize = 1 << 18; // NSEventModifierFlagControl
const OPTION: usize = 1 << 19; // NSEventModifierFlagOption

unsafe fn ns_string(text: &str) -> Retained<AnyObject> {
    let text = CString::new(text).expect("no interior NUL");
    msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()]
}

unsafe fn add_item(
    menu: &AnyObject,
    target: &MenuTarget,
    title: &str,
    action: MenuAction,
    key: &str,
    checked: Option<bool>,
    enabled: bool,
) {
    let title = ns_string(title);
    let key_equivalent = ns_string(key);
    let selector: Sel = sel!(menuAction:);
    let item: Allocated<AnyObject> = msg_send![class!(NSMenuItem), alloc];
    let item: Retained<AnyObject> =
        msg_send![item, initWithTitle: &*title, action: selector, keyEquivalent: &*key_equivalent];
    if !key.is_empty() {
        // Shown as ⌃⌥<key>: the viewer's own hotkeys.
        let _: () = msg_send![&*item, setKeyEquivalentModifierMask: CONTROL | OPTION];
    }
    let _: () = msg_send![&*item, setTarget: target];
    let _: () = msg_send![&*item, setTag: action.tag()];
    if let Some(checked) = checked {
        let _: () = msg_send![&*item, setState: if checked { 1isize } else { 0isize }];
    }
    let _: () = msg_send![&*item, setEnabled: Bool::new(enabled)];
    let _: () = msg_send![menu, addItem: &*item];
}

/// A greyed-out label heading a group of items.
unsafe fn add_heading(menu: &AnyObject, title: &str) {
    let title = ns_string(title);
    let empty = ns_string("");
    let item: Allocated<AnyObject> = msg_send![class!(NSMenuItem), alloc];
    let nothing: Option<Sel> = None;
    let item: Retained<AnyObject> =
        msg_send![item, initWithTitle: &*title, action: nothing, keyEquivalent: &*empty];
    let _: () = msg_send![&*item, setEnabled: Bool::NO];
    let _: () = msg_send![menu, addItem: &*item];
}

unsafe fn add_submenu(menu: &AnyObject, title: &str, submenu: &AnyObject) {
    let title = ns_string(title);
    let empty = ns_string("");
    let item: Allocated<AnyObject> = msg_send![class!(NSMenuItem), alloc];
    let nothing: Option<Sel> = None;
    let item: Retained<AnyObject> =
        msg_send![item, initWithTitle: &*title, action: nothing, keyEquivalent: &*empty];
    let _: () = msg_send![&*item, setSubmenu: submenu];
    let _: () = msg_send![menu, addItem: &*item];
}

unsafe fn video_menu(target: &MenuTarget, state: &MenuState) -> Retained<AnyObject> {
    let menu: Retained<AnyObject> = msg_send![class!(NSMenu), new];
    let _: () = msg_send![&*menu, setAutoenablesItems: Bool::NO];
    set_dark(&menu);
    let on = state.video_available;
    add_heading(&menu, "Codec");
    for codec in [Codec::Hevc, Codec::H264] {
        let title = match codec {
            Codec::Hevc => "HEVC (sharper per bit)",
            Codec::H264 => "H.264 (most compatible)",
        };
        let checked = state.codec == codec.name();
        add_item(
            &menu,
            target,
            title,
            MenuAction::Codec(codec),
            "",
            Some(checked),
            on,
        );
    }
    add_separator(&menu);
    add_heading(&menu, "Resolution");
    for percent in SCALES {
        let title = if percent == 100 && state.scale == 100 {
            format!("Native ({}×{})", state.stream_size.0, state.stream_size.1)
        } else if percent == 100 {
            "Native".to_string()
        } else {
            format!("{percent}% (less to send, softer)")
        };
        let checked = state.scale == percent;
        add_item(
            &menu,
            target,
            &title,
            MenuAction::Scale(percent),
            "",
            Some(checked),
            on,
        );
    }
    add_separator(&menu);
    add_heading(&menu, "Frame rate");
    for fps in FRAME_RATES {
        let title = if fps == 30 {
            "30 fps (half the work for slow computers)".to_string()
        } else {
            format!("{fps} fps")
        };
        add_item(
            &menu,
            target,
            &title,
            MenuAction::FrameRate(fps),
            "",
            Some(state.fps == fps),
            on,
        );
    }
    add_separator(&menu);
    add_heading(&menu, "Bitrate limit");
    for mbps in BITRATES_MBPS {
        let checked = state.bitrate_mbps == Some(mbps);
        add_item(
            &menu,
            target,
            &format!("{mbps} Mbps"),
            MenuAction::BitrateMbps(mbps),
            "",
            Some(checked),
            on,
        );
    }
    add_separator(&menu);
    add_item(
        &menu,
        target,
        "Fast Lane (lossless text while typing)",
        MenuAction::ToggleFastLane,
        "",
        Some(state.fast_lane),
        on,
    );
    menu
}

unsafe fn keyboard_menu(target: &MenuTarget, state: &MenuState) -> Retained<AnyObject> {
    let menu: Retained<AnyObject> = msg_send![class!(NSMenu), new];
    let _: () = msg_send![&*menu, setAutoenablesItems: Bool::NO];
    set_dark(&menu);
    add_item(
        &menu,
        target,
        "Send ⌘ Shortcuts to Remote",
        MenuAction::ToggleCapture,
        "g",
        Some(state.capture),
        state.capture_available,
    );
    add_separator(&menu);
    add_heading(&menu, "Send keys");
    for (index, shortcut) in state.shortcuts.iter().enumerate() {
        add_item(
            &menu,
            target,
            shortcut.title,
            MenuAction::SendShortcut(index as u8),
            "",
            None,
            true,
        );
    }
    menu
}

unsafe fn clipboard_menu(target: &MenuTarget, state: &MenuState) -> Retained<AnyObject> {
    let menu: Retained<AnyObject> = msg_send![class!(NSMenu), new];
    let _: () = msg_send![&*menu, setAutoenablesItems: Bool::NO];
    set_dark(&menu);
    add_item(
        &menu,
        target,
        "Share Clipboard",
        MenuAction::ToggleClipboard,
        "",
        Some(state.clipboard),
        true,
    );
    add_item(
        &menu,
        target,
        "Type Clipboard Text",
        MenuAction::TypeClipboard,
        "",
        None,
        true,
    );
    menu
}

/// This Mac's clipboard as text, for typing it out on the host.
pub fn clipboard_text() -> Option<String> {
    // SAFETY: plain AppKit calls on the main thread (menu actions run there).
    unsafe {
        let pasteboard: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        if pasteboard.is_null() {
            return None;
        }
        let kind = ns_string("public.utf8-plain-text");
        let text: *mut AnyObject = msg_send![pasteboard, stringForType: &*kind];
        if text.is_null() {
            return None;
        }
        let utf8: *const std::ffi::c_char = msg_send![text, UTF8String];
        (!utf8.is_null()).then(|| {
            std::ffi::CStr::from_ptr(utf8)
                .to_string_lossy()
                .into_owned()
        })
    }
}

unsafe fn add_separator(menu: &AnyObject) {
    let separator: Retained<AnyObject> = msg_send![class!(NSMenuItem), separatorItem];
    let _: () = msg_send![menu, addItem: &*separator];
}

/// Dark like the video around it, whatever the Mac's own appearance.
unsafe fn set_dark(menu: &AnyObject) {
    let dark = ns_string("NSAppearanceNameDarkAqua");
    let appearance: *mut AnyObject = msg_send![class!(NSAppearance), appearanceNamed: &*dark];
    if !appearance.is_null() {
        let _: () = msg_send![menu, setAppearance: appearance];
    }
}

unsafe fn show(job: &PopUp) {
    let target: Retained<MenuTarget> = msg_send![MenuTarget::class(), new];
    let menu: Retained<AnyObject> = msg_send![class!(NSMenu), new];
    // Items are enabled explicitly; don't let AppKit re-validate them.
    let _: () = msg_send![&*menu, setAutoenablesItems: Bool::NO];
    set_dark(&menu);
    let state = &job.state;
    add_heading(&menu, &state.host);
    add_heading(&menu, &state.detail);
    add_separator(&menu);
    let screen = if state.fullscreen {
        "Windowed"
    } else {
        "Full Screen"
    };
    add_item(
        &menu,
        &target,
        screen,
        MenuAction::ToggleFullscreen,
        "f",
        None,
        true,
    );
    add_item(
        &menu,
        &target,
        "Stats Bar",
        MenuAction::ToggleStats,
        "s",
        Some(state.stats),
        true,
    );
    add_separator(&menu);
    let keyboard = keyboard_menu(&target, state);
    add_submenu(&menu, "Keyboard", &keyboard);
    let clipboard = clipboard_menu(&target, state);
    add_submenu(&menu, "Clipboard", &clipboard);
    let video = video_menu(&target, state);
    add_submenu(&menu, "Video", &video);
    add_separator(&menu);
    let button = if state.button_visible {
        "Hide Menu Button"
    } else {
        "Show Menu Button"
    };
    add_item(
        &menu,
        &target,
        button,
        MenuAction::HideButton,
        "",
        None,
        true,
    );
    add_separator(&menu);
    add_item(
        &menu,
        &target,
        "Disconnect",
        MenuAction::Disconnect,
        "q",
        None,
        true,
    );

    let location = CGPointRepr {
        x: job.location.0,
        y: job.location.1,
    };
    let nil: *mut AnyObject = std::ptr::null_mut();
    let _: Bool =
        msg_send![&*menu, popUpMenuPositioningItem: nil, atLocation: location, inView: job.view];
    // The target must live until tracking ends, which is now.
    drop(target);
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPointRepr {
    x: f64,
    y: f64,
}

// SAFETY: matches NSPoint/CGPoint on 64-bit.
unsafe impl Encode for CGPointRepr {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}
