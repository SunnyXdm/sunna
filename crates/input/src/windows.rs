//! Windows input uses physical scan codes so games see the same keys as the desktop.

use std::collections::HashSet;

use sunna_proto::messages::{GestureKind, GesturePhase, InputEvent, MouseButton};
use windows::Win32::Foundation::{GetLastError, SetLastError, WIN32_ERROR};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

use crate::windows_mapping::{absolute, accumulate, scan_from_mac, scroll, Scan};
use crate::InputInjector;

const CONTROL: Scan = Scan {
    code: 0x1d,
    extended: false,
};
const RIGHT_CONTROL: Scan = Scan {
    code: 0x1d,
    extended: true,
};
const PINCH_STEP: f32 = 0.125;

pub struct WinInjector {
    keys_down: HashSet<Scan>,
    buttons_down: HashSet<u8>,
    motion_remainder: (f32, f32),
    scroll_remainder: (f32, f32),
    pinch: f32,
    command_as_ctrl: bool,
    failures: u64,
}

impl Default for WinInjector {
    fn default() -> Self {
        Self::new()
    }
}

impl WinInjector {
    pub fn new() -> Self {
        let command_as_ctrl = std::env::var("SUNNA_MAC_COMMAND").is_ok_and(|value| value == "ctrl");
        tracing::info!(command_as_ctrl, "Windows input injection (SendInput)");
        Self {
            keys_down: HashSet::new(),
            buttons_down: HashSet::new(),
            motion_remainder: (0.0, 0.0),
            scroll_remainder: (0.0, 0.0),
            pinch: 0.0,
            command_as_ctrl,
            failures: 0,
        }
    }

    fn send(&mut self, actions: &[Action]) -> anyhow::Result<()> {
        if actions.is_empty() {
            return Ok(());
        }
        let inputs: Vec<_> = actions.iter().map(Action::input).collect();
        let send = || {
            // SAFETY: INPUT unions are initialized according to their type and the
            // slice remains valid throughout the call. Clear stale last-error first:
            // UIPI can reject input without setting an error code.
            unsafe {
                SetLastError(WIN32_ERROR(0));
                let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) as usize;
                (sent, GetLastError())
            }
        };
        let (mut sent, mut error) = send();
        // Input only reaches the desktop that has the keyboard. When the lock
        // screen or a UAC prompt comes up (or goes away) that changes: move
        // to it and try again. Only works when running as SYSTEM.
        if sent == 0 && follow_input_desktop() {
            (sent, error) = send();
        }
        // Only record the accepted prefix, including temporary pinch modifiers.
        // Failed releases remain tracked so release_all can retry them.
        for action in actions.iter().take(sent) {
            match *action {
                Action::Key(key, true) => {
                    self.keys_down.insert(key);
                }
                Action::Key(key, false) => {
                    self.keys_down.remove(&key);
                }
                Action::Button(button, true) => {
                    self.buttons_down.insert(button);
                }
                Action::Button(button, false) => {
                    self.buttons_down.remove(&button);
                }
                Action::Mouse(_) => {}
            }
        }
        if sent != inputs.len() {
            self.failures = self.failures.saturating_add(1);
            if self.failures.is_power_of_two() {
                tracing::warn!(
                    failures = self.failures,
                    sent,
                    requested = inputs.len(),
                    error = error.0,
                    "SendInput failed; UIPI or the secure desktop may block injection"
                );
            }
            anyhow::bail!("SendInput accepted {sent}/{} events; GetLastError={} (UIPI/secure desktop may block input without an error code)", inputs.len(), error.0);
        }
        Ok(())
    }
}

impl InputInjector for WinInjector {
    fn inject(&mut self, event: &InputEvent) -> anyhow::Result<()> {
        let mut actions = Vec::with_capacity(3);
        match *event {
            InputEvent::Key {
                scancode,
                pressed,
                repeat,
            } => {
                let Some(key) = scan_from_mac(scancode, self.command_as_ctrl) else {
                    return Ok(());
                };
                if (!pressed || repeat) && !self.keys_down.contains(&key) {
                    return Ok(());
                }
                // XTEST supplies its own repeat; SendInput doesn't. Replay the
                // viewer's repeat keydowns, without changing our held-key set.
                actions.push(Action::Key(key, pressed));
            }
            InputEvent::MouseMoveAbs { x, y } => {
                let screen = target_rect();
                // SAFETY: these metrics take no pointers and are read afresh in
                // case the desktop geometry changed since the session started.
                let (x, y) = unsafe {
                    (
                        absolute(
                            x,
                            screen.left,
                            screen.right - screen.left,
                            GetSystemMetrics(SM_XVIRTUALSCREEN),
                            GetSystemMetrics(SM_CXVIRTUALSCREEN),
                        ),
                        absolute(
                            y,
                            screen.top,
                            screen.bottom - screen.top,
                            GetSystemMetrics(SM_YVIRTUALSCREEN),
                            GetSystemMetrics(SM_CYVIRTUALSCREEN),
                        ),
                    )
                };
                actions.push(mouse(
                    MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                    x,
                    y,
                    0,
                ));
            }
            InputEvent::MouseMoveRel { dx, dy } => {
                let x = accumulate(dx, &mut self.motion_remainder.0);
                let y = accumulate(dy, &mut self.motion_remainder.1);
                if x != 0 || y != 0 {
                    actions.push(mouse(MOUSEEVENTF_MOVE, x, y, 0));
                }
            }
            InputEvent::MouseButton { button, pressed } => {
                let button = match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                    MouseButton::X1 => 3,
                    MouseButton::X2 => 4,
                };
                if pressed || self.buttons_down.contains(&button) {
                    actions.push(Action::Button(button, pressed));
                }
            }
            InputEvent::Scroll { dx, dy, phase, .. } => {
                if matches!(phase, Some(GesturePhase::Begin)) {
                    self.scroll_remainder = (0.0, 0.0);
                }
                let (x, y) = scroll(dx, dy, &mut self.scroll_remainder);
                if y != 0 {
                    actions.push(mouse(MOUSEEVENTF_WHEEL, 0, 0, y as u32));
                }
                if x != 0 {
                    actions.push(mouse(MOUSEEVENTF_HWHEEL, 0, 0, x as u32));
                }
                if matches!(phase, Some(GesturePhase::End | GesturePhase::Cancel)) {
                    self.scroll_remainder = (0.0, 0.0);
                }
            }
            InputEvent::Gesture {
                kind: GestureKind::Pinch,
                phase,
                scale_delta,
                ..
            } => {
                if phase == GesturePhase::Begin {
                    self.pinch = 0.0;
                }
                if scale_delta.is_finite() {
                    self.pinch += scale_delta;
                }
                let steps = (self.pinch / PINCH_STEP).trunc();
                self.pinch -= steps * PINCH_STEP;
                if steps != 0.0 {
                    let held = self.keys_down.contains(&CONTROL)
                        || self.keys_down.contains(&RIGHT_CONTROL);
                    if !held {
                        actions.push(Action::Key(CONTROL, true));
                    }
                    actions.push(mouse(
                        MOUSEEVENTF_WHEEL,
                        0,
                        0,
                        (steps * 120.0) as i32 as u32,
                    ));
                    if !held {
                        actions.push(Action::Key(CONTROL, false));
                    }
                }
                if matches!(phase, GesturePhase::End | GesturePhase::Cancel) {
                    self.pinch = 0.0;
                }
            }
            // Windows has no equivalent portable injection for these semantic gestures.
            InputEvent::Gesture { .. } => {}
        }
        self.send(&actions)
    }

    fn release_all(&mut self) {
        let mut actions: Vec<_> = self
            .keys_down
            .iter()
            .map(|&key| Action::Key(key, false))
            .collect();
        actions.extend(
            self.buttons_down
                .iter()
                .map(|&button| Action::Button(button, false)),
        );
        let _ = self.send(&actions);
        self.motion_remainder = (0.0, 0.0);
        self.scroll_remainder = (0.0, 0.0);
        self.pinch = 0.0;
    }
}

impl Drop for WinInjector {
    fn drop(&mut self) {
        self.release_all();
    }
}

enum Action {
    Key(Scan, bool),
    Button(u8, bool),
    Mouse(MOUSEINPUT),
}

fn mouse(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: u32) -> Action {
    Action::Mouse(MOUSEINPUT {
        dx,
        dy,
        mouseData: data,
        dwFlags: flags,
        ..Default::default()
    })
}

impl Action {
    fn input(&self) -> INPUT {
        let mi = match *self {
            Self::Key(key, pressed) => {
                let mut flags = KEYEVENTF_SCANCODE;
                if key.extended {
                    flags |= KEYEVENTF_EXTENDEDKEY;
                }
                if !pressed {
                    flags |= KEYEVENTF_KEYUP;
                }
                return INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wScan: key.code,
                            dwFlags: flags,
                            ..Default::default()
                        },
                    },
                };
            }
            Self::Button(button, pressed) => {
                let (down, up, data) = match button {
                    0 => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0),
                    1 => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
                    2 => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
                    3 => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, 1),
                    _ => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, 2),
                };
                MOUSEINPUT {
                    mouseData: data,
                    dwFlags: if pressed { down } else { up },
                    ..Default::default()
                }
            }
            Self::Mouse(mi) => mi,
        };
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 { mi },
        }
    }
}

/// Switch this thread to the desktop that has the keyboard (the normal one,
/// or the secure one with the lock screen and UAC prompts). Whether it
/// worked; it can't for a thread that owns windows, or without SYSTEM rights
/// on the secure desktop.
/// The display the stream shows, by GDI name (`\\.\DISPLAY7`), when it
/// isn't the primary one (Sunna's own virtual display next to a monitor):
/// the pointer goes there. With its rectangle as last looked up.
static TARGET: std::sync::Mutex<(Option<String>, Option<(RECT, std::time::Instant)>)> =
    std::sync::Mutex::new((None, None));

/// Send the pointer to `name` (None: the primary display).
pub fn target_display(name: Option<String>) {
    *TARGET.lock().unwrap() = (name, None);
}

/// The target display's rectangle on the desktop, looked up at most twice
/// a second (pointer moves come fast; displays rarely move).
fn target_rect() -> RECT {
    // SAFETY: plain metrics.
    let primary = unsafe { RECT { left: 0, top: 0, right: GetSystemMetrics(SM_CXSCREEN), bottom: GetSystemMetrics(SM_CYSCREEN) } };
    let mut target = TARGET.lock().unwrap();
    let Some(name) = target.0.clone() else {
        return primary;
    };
    if let Some((rect, when)) = target.1 {
        if when.elapsed() < std::time::Duration::from_millis(500) {
            return rect;
        }
    }
    let rect = display_rect(&name).unwrap_or(primary);
    target.1 = Some((rect, std::time::Instant::now()));
    rect
}

fn display_rect(name: &str) -> Option<RECT> {
    struct Search<'a> {
        name: &'a str,
        found: Option<RECT>,
    }
    unsafe extern "system" fn visit(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> windows::core::BOOL {
        // SAFETY: `data` is the Search below; the info struct's size is set.
        unsafe {
            let search = &mut *(data.0 as *mut Search);
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(monitor, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO).as_bool() {
                let end = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
                if String::from_utf16_lossy(&info.szDevice[..end]) == search.name {
                    search.found = Some(info.monitorInfo.rcMonitor);
                    return false.into();
                }
            }
            true.into()
        }
    }
    let mut search = Search { name, found: None };
    // SAFETY: the callback only touches `search`, which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(visit), LPARAM(&mut search as *mut Search as isize));
    }
    search.found
}

fn follow_input_desktop() -> bool {
    use windows::Win32::System::StationsAndDesktops::{
        OpenInputDesktop, SetThreadDesktop, DESKTOP_ACCESS_FLAGS, DESKTOP_CONTROL_FLAGS,
    };
    const GENERIC_ALL: u32 = 0x1000_0000;
    // SAFETY: the desktop handle stays open while it's this thread's desktop
    // (closing it would fail anyway); a few handles over a session's
    // desktop switches is fine.
    unsafe {
        OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_ACCESS_FLAGS(GENERIC_ALL))
            .is_ok_and(|desktop| SetThreadDesktop(desktop).is_ok())
    }
}
