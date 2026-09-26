//! Raw Mac keyboard events → what goes to the host, plus the viewer's own
//! hotkeys. Platform-independent so it's unit-tested anywhere; the macOS
//! event tap (`mac_keyboard`) feeds it.
//!
//! What it fixes over forwarding the window's key events as they come:
//! - Caps Lock: macOS reports it only as a state change, so it's sent as a
//!   press + release, which toggles it on the host.
//! - Modifiers are re-checked against the event's flags on every event and
//!   released if the Mac says they're up, so a missed key-up (focus change,
//!   system dialog) can't leave Shift or Cmd stuck on the host.
//! - ⌃⌥ + S/F/G/Q/M are the viewer's own and never reach the host, including
//!   their key-ups.

// Only the hotkeys are used where there's no event tap (non-macOS).
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use sunna_proto::messages::InputEvent;

/// Carbon virtual keycodes the translator cares about.
mod vk {
    pub const S: u16 = 0x01;
    pub const F: u16 = 0x03;
    pub const G: u16 = 0x05;
    pub const Q: u16 = 0x0C;
    pub const M: u16 = 0x2E;
    pub const CAPS_LOCK: u16 = 0x39;
    pub const FUNCTION: u16 = 0x3F;
}

/// `CGEventFlags` bits.
pub mod flags {
    #[cfg(test)]
    pub const CAPS_LOCK: u64 = 0x0001_0000;
    #[cfg(test)]
    pub const SHIFT: u64 = 0x0002_0000;
    pub const CONTROL: u64 = 0x0004_0000;
    pub const OPTION: u64 = 0x0008_0000;
    pub const COMMAND: u64 = 0x0010_0000;
}

/// Left/right modifier keys and their device-dependent flag bits (IOKit's
/// NX_DEVICE*KEYMASK), which say whether that particular key is down.
const MODIFIERS: [(u16, u64); 8] = [
    (0x3B, 0x0000_0001), // left Control
    (0x38, 0x0000_0002), // left Shift
    (0x3C, 0x0000_0004), // right Shift
    (0x37, 0x0000_0008), // left Command
    (0x36, 0x0000_0010), // right Command
    (0x3A, 0x0000_0020), // left Option
    (0x3D, 0x0000_0040), // right Option
    (0x3E, 0x0000_2000), // right Control
];

fn modifier_mask(keycode: u16) -> Option<u64> {
    MODIFIERS
        .iter()
        .find(|(code, _)| *code == keycode)
        .map(|(_, mask)| *mask)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawKind {
    Down,
    Up,
    FlagsChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    /// ⌃⌥S
    ToggleStats,
    /// ⌃⌥F
    ToggleFullscreen,
    /// ⌃⌥G: hand ⌘Tab and friends back to macOS, or take them again.
    ToggleCapture,
    /// ⌃⌥Q
    Disconnect,
    /// ⌃⌥M: the in-session menu.
    Menu,
}

impl Hotkey {
    /// The hotkey for a key pressed with `flags`, if it is one: Control and
    /// Option held, Command not.
    pub fn from_key(keycode: u16, flags: u64) -> Option<Self> {
        let chord = flags & (flags::CONTROL | flags::OPTION | flags::COMMAND)
            == flags::CONTROL | flags::OPTION;
        if !chord {
            return None;
        }
        match keycode {
            vk::S => Some(Self::ToggleStats),
            vk::F => Some(Self::ToggleFullscreen),
            vk::G => Some(Self::ToggleCapture),
            vk::Q => Some(Self::Disconnect),
            vk::M => Some(Self::Menu),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Send(InputEvent),
    Hotkey(Hotkey),
}

fn key(scancode: u16, pressed: bool, repeat: bool) -> Action {
    Action::Send(InputEvent::Key {
        scancode,
        pressed,
        repeat,
    })
}

#[derive(Default)]
pub struct Translator {
    /// Keys the host currently has down because of us, in press order.
    down: Vec<u16>,
    /// Keys whose press was a viewer hotkey: their key-up is swallowed too.
    hotkey_keys: Vec<u16>,
}

impl Translator {
    pub fn handle(&mut self, kind: RawKind, keycode: u16, flags: u64, repeat: bool) -> Vec<Action> {
        // A key is the viewer's from its first press to its release, repeats
        // included, whatever the modifiers do in between.
        if self.hotkey_keys.contains(&keycode) && kind != RawKind::FlagsChanged {
            if kind == RawKind::Up {
                self.take_hotkey_up(keycode);
            }
            return self.resync_modifiers(flags, keycode, false);
        }
        let hotkey = (kind == RawKind::Down && !repeat && !self.down.contains(&keycode))
            .then(|| Hotkey::from_key(keycode, flags))
            .flatten();
        // Before a real key goes out, the host's modifiers must match the
        // Mac's (Shift held across a capture change, say). Only then: a lone
        // modifier press sent on its own could open a menu or launcher.
        let press_modifiers = kind == RawKind::Down && hotkey.is_none();
        let mut out = self.resync_modifiers(flags, keycode, press_modifiers);
        match kind {
            RawKind::FlagsChanged => {
                if keycode == vk::CAPS_LOCK {
                    // One event per toggle, no matching "up": tap it.
                    out.push(key(keycode, true, false));
                    out.push(key(keycode, false, false));
                } else if let Some(mask) = modifier_mask(keycode) {
                    let pressed = flags & mask != 0;
                    if pressed && !self.down.contains(&keycode) {
                        self.track(keycode, true);
                        out.push(key(keycode, true, false));
                    } else if !pressed {
                        // Sent even if we never saw it go down: it may have
                        // gone down through the window's own key events
                        // (capture off, or the tap briefly disabled).
                        self.track(keycode, false);
                        out.push(key(keycode, false, false));
                    }
                }
                // Fn and unknown modifiers stay on the Mac.
            }
            RawKind::Down => {
                if keycode == vk::FUNCTION {
                    return out;
                }
                if let Some(hotkey) = hotkey {
                    self.hotkey_keys.push(keycode);
                    out.push(Action::Hotkey(hotkey));
                    return out;
                }
                self.track(keycode, true);
                out.push(key(keycode, true, repeat));
            }
            RawKind::Up => {
                // Forwarded even if we never saw the press: it may have gone
                // down through the window's own key events, and a stray
                // release is harmless where a stuck key is not.
                self.track(keycode, false);
                out.push(key(keycode, false, false));
            }
        }
        out
    }

    /// Release everything the host has down because of us: non-modifiers
    /// first, while their modifiers still apply.
    pub fn release_all(&mut self) -> Vec<Action> {
        let mut keys = std::mem::take(&mut self.down);
        keys.sort_by_key(|&code| modifier_mask(code).is_some());
        keys.into_iter()
            .map(|code| key(code, false, false))
            .collect()
    }

    /// Whether `keycode` is held as a viewer hotkey: its repeats and key-up
    /// are ours too, even after capture was released mid-chord.
    pub fn owns(&self, keycode: u16) -> bool {
        self.hotkey_keys.contains(&keycode)
    }

    /// If `keycode`'s press was a viewer hotkey, forget it and say so.
    pub fn take_hotkey_up(&mut self, keycode: u16) -> bool {
        match self.hotkey_keys.iter().position(|&code| code == keycode) {
            Some(index) => {
                self.hotkey_keys.swap_remove(index);
                true
            }
            None => false,
        }
    }

    /// Forget a hotkey key-up we were waiting for; used when a key-up can
    /// no longer reach us (focus left the window).
    pub fn forget_hotkeys(&mut self) {
        self.hotkey_keys.clear();
    }

    /// Make the host's modifiers match `flags`: release any we believe are
    /// down whose bit is clear (released while we weren't looking) and, if
    /// `press`, press any whose bit is set that the host doesn't have.
    fn resync_modifiers(&mut self, flags: u64, current: u16, press: bool) -> Vec<Action> {
        let mut out = Vec::new();
        for (code, mask) in MODIFIERS {
            if code == current {
                continue;
            }
            let held = flags & mask != 0;
            let host_has = self.down.contains(&code);
            if host_has && !held {
                self.track(code, false);
                out.push(key(code, false, false));
            } else if press && held && !host_has {
                self.track(code, true);
                out.push(key(code, true, false));
            }
        }
        out
    }

    fn track(&mut self, keycode: u16, pressed: bool) {
        if pressed {
            if !self.down.contains(&keycode) {
                self.down.push(keycode);
            }
        } else {
            self.down.retain(|&code| code != keycode);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L_SHIFT: u16 = 0x38;
    const L_CMD: u16 = 0x37;
    const L_CTRL: u16 = 0x3B;
    const L_OPT: u16 = 0x3A;
    const A: u16 = 0x00;

    fn sent(actions: &[Action]) -> Vec<(u16, bool)> {
        actions
            .iter()
            .filter_map(|action| match action {
                Action::Send(InputEvent::Key {
                    scancode, pressed, ..
                }) => Some((*scancode, *pressed)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn caps_lock_toggles() {
        let mut t = Translator::default();
        let on = t.handle(
            RawKind::FlagsChanged,
            vk::CAPS_LOCK,
            flags::CAPS_LOCK,
            false,
        );
        assert_eq!(sent(&on), [(vk::CAPS_LOCK, true), (vk::CAPS_LOCK, false)]);
        let off = t.handle(RawKind::FlagsChanged, vk::CAPS_LOCK, 0, false);
        assert_eq!(sent(&off), [(vk::CAPS_LOCK, true), (vk::CAPS_LOCK, false)]);
        assert!(t.release_all().is_empty());
    }

    #[test]
    fn modifiers_follow_device_bits() {
        let mut t = Translator::default();
        let down = t.handle(RawKind::FlagsChanged, L_SHIFT, flags::SHIFT | 0x2, false);
        assert_eq!(sent(&down), [(L_SHIFT, true)]);
        let up = t.handle(RawKind::FlagsChanged, L_SHIFT, 0, false);
        assert_eq!(sent(&up), [(L_SHIFT, false)]);
    }

    #[test]
    fn missed_modifier_release_is_resynced() {
        let mut t = Translator::default();
        t.handle(RawKind::FlagsChanged, L_CMD, flags::COMMAND | 0x8, false);
        // Cmd's key-up went elsewhere; the next key arrives without it.
        let actions = t.handle(RawKind::Down, A, 0, false);
        assert_eq!(sent(&actions), [(L_CMD, false), (A, true)]);
    }

    #[test]
    fn hotkeys_never_reach_the_host() {
        let mut t = Translator::default();
        let ctrl_opt = flags::CONTROL | flags::OPTION | 0x1 | 0x20;
        t.handle(RawKind::FlagsChanged, L_CTRL, flags::CONTROL | 0x1, false);
        t.handle(RawKind::FlagsChanged, L_OPT, ctrl_opt, false);
        let press = t.handle(RawKind::Down, vk::G, ctrl_opt, false);
        assert_eq!(press, [Action::Hotkey(Hotkey::ToggleCapture)]);
        assert!(t.handle(RawKind::Down, vk::G, ctrl_opt, true).is_empty());
        // Modifiers released before the letter: its up is still ours.
        t.handle(RawKind::FlagsChanged, L_OPT, flags::CONTROL | 0x1, false);
        t.handle(RawKind::FlagsChanged, L_CTRL, 0, false);
        assert!(t.handle(RawKind::Up, vk::G, 0, false).is_empty());
    }

    #[test]
    fn command_chords_are_not_hotkeys() {
        let mut t = Translator::default();
        let flags = flags::CONTROL | flags::OPTION | flags::COMMAND;
        let actions = t.handle(RawKind::Down, vk::Q, flags, false);
        assert_eq!(sent(&actions), [(vk::Q, true)]);
    }

    #[test]
    fn release_all_lets_go_of_letters_before_modifiers() {
        let mut t = Translator::default();
        t.handle(RawKind::FlagsChanged, L_CMD, flags::COMMAND | 0x8, false);
        t.handle(RawKind::Down, A, flags::COMMAND | 0x8, false);
        assert_eq!(sent(&t.release_all()), [(A, false), (L_CMD, false)]);
        assert!(t.release_all().is_empty());
    }

    #[test]
    fn hotkey_repeats_stay_with_whoever_owned_the_press() {
        let mut t = Translator::default();
        let ctrl_opt = flags::CONTROL | flags::OPTION | 0x1 | 0x20;
        // S held first, then ⌃⌥ added: S's repeats and release are the host's.
        t.handle(RawKind::Down, vk::S, 0, false);
        let repeat = t.handle(RawKind::Down, vk::S, ctrl_opt, true);
        assert!(!repeat
            .iter()
            .any(|action| matches!(action, Action::Hotkey(_))));
        assert_eq!(
            sent(&t.handle(RawKind::Up, vk::S, ctrl_opt, false)),
            [(vk::S, false)]
        );
        // ⌃⌥G: G is the viewer's through its repeats, even with ⌃⌥ let go.
        assert_eq!(
            t.handle(RawKind::Down, vk::G, ctrl_opt, false),
            [Action::Hotkey(Hotkey::ToggleCapture)]
        );
        assert!(t.owns(vk::G));
        // (The host does get ⌃⌥'s releases.)
        let no_g = |actions: Vec<Action>| !sent(&actions).iter().any(|(code, _)| *code == vk::G);
        assert!(no_g(t.handle(RawKind::Down, vk::G, 0, true)));
        assert!(no_g(t.handle(RawKind::Up, vk::G, 0, false)));
        assert!(!t.owns(vk::G));
    }

    #[test]
    fn held_modifier_is_pressed_before_the_next_key() {
        let mut t = Translator::default();
        // Shift went down while capture was off; the next key must be shifted.
        let actions = t.handle(RawKind::Down, A, flags::SHIFT | 0x2, false);
        assert_eq!(sent(&actions), [(L_SHIFT, true), (A, true)]);
        // A lone modifier change never presses others (no stray Super taps).
        let mut t = Translator::default();
        let actions = t.handle(
            RawKind::FlagsChanged,
            L_SHIFT,
            flags::COMMAND | 0x8 | flags::SHIFT | 0x2,
            false,
        );
        assert_eq!(sent(&actions), [(L_SHIFT, true)]);
    }

    #[test]
    fn untracked_releases_still_reach_the_host() {
        let mut t = Translator::default();
        // Pressed through the window's key events while the tap was off.
        assert_eq!(sent(&t.handle(RawKind::Up, A, 0, false)), [(A, false)]);
        assert_eq!(
            sent(&t.handle(RawKind::FlagsChanged, L_SHIFT, 0, false)),
            [(L_SHIFT, false)]
        );
    }

    #[test]
    fn repeats_pass_through() {
        let mut t = Translator::default();
        t.handle(RawKind::Down, A, 0, false);
        let repeat = t.handle(RawKind::Down, A, 0, true);
        assert_eq!(
            repeat,
            [Action::Send(InputEvent::Key {
                scancode: A,
                pressed: true,
                repeat: true
            })]
        );
    }
}
