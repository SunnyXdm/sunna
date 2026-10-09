use crate::session::{self, Session};
use std::collections::HashSet;
use std::sync::Arc;
use sunna_input::{linux::evdev_from_mac, InputInjector};
use sunna_proto::messages::{GestureKind, GesturePhase, InputEvent, MouseButton};

pub struct PortalInjector {
    session: Arc<Session>,
    cursor: (f64, f64),
    keys: HashSet<i32>,
    buttons: HashSet<i32>,
    pinch: f32,
    wheel: (f32, f32),
    command_as_ctrl: bool,
}
fn button_code(button: MouseButton) -> i32 {
    match button {
        MouseButton::Left => 0x110,
        MouseButton::Right => 0x111,
        MouseButton::Middle => 0x112,
        MouseButton::X1 => 0x113,
        MouseButton::X2 => 0x114,
    }
}
fn coordinates(x: f64, y: f64, size: (u32, u32)) -> (f64, f64) {
    let clamp = |n: f64, max: u32| {
        if n.is_finite() {
            n.clamp(0.0, max.saturating_sub(1) as f64)
        } else {
            0.0
        }
    };
    (clamp(x, size.0), clamp(y, size.1))
}
// The viewer converts one wheel line to 32 pixels on the wire.
fn wheel_steps(delta: f32, remainder: &mut f32) -> i32 {
    let clicks = delta / 32.0 + *remainder;
    let whole = clicks.trunc();
    *remainder = clicks - whole;
    (whole as i32).saturating_neg()
}
fn scroll_finished(phase: Option<GesturePhase>) -> bool {
    matches!(phase, Some(GesturePhase::End | GesturePhase::Cancel))
}
impl PortalInjector {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            session: session::acquire()?,
            cursor: (0.0, 0.0),
            keys: HashSet::new(),
            buttons: HashSet::new(),
            pinch: 0.0,
            wheel: (0.0, 0.0),
            command_as_ctrl: std::env::var("SUNNA_MAC_COMMAND").is_ok_and(|v| v == "ctrl"),
        })
    }
    fn move_to(&mut self, x: f64, y: f64) -> anyhow::Result<()> {
        self.cursor = coordinates(x, y, self.session.stream.size);
        self.session.motion(self.cursor.0, self.cursor.1)
    }
    fn key(&mut self, code: i32, pressed: bool) -> anyhow::Result<()> {
        if !pressed && !self.keys.contains(&code) {
            return Ok(());
        }
        self.session.key(code, pressed)?;
        if pressed {
            self.keys.insert(code);
        } else {
            self.keys.remove(&code);
        }
        Ok(())
    }
}
impl InputInjector for PortalInjector {
    fn inject(&mut self, event: &InputEvent) -> anyhow::Result<()> {
        match *event {
            InputEvent::MouseMoveAbs { x, y } => self.move_to(
                x as f64 * self.session.stream.size.0 as f64,
                y as f64 * self.session.stream.size.1 as f64,
            )?,
            InputEvent::MouseMoveRel { dx, dy } => {
                self.move_to(self.cursor.0 + dx as f64, self.cursor.1 + dy as f64)?
            }
            InputEvent::MouseButton { button, pressed } => {
                let code = button_code(button);
                if pressed || self.buttons.contains(&code) {
                    self.session.button(code, pressed)?;
                    if pressed {
                        self.buttons.insert(code);
                    } else {
                        self.buttons.remove(&code);
                    }
                }
            }
            InputEvent::Scroll { dx, dy, phase, .. } => {
                if dx.is_finite() && dy.is_finite() {
                    // Portal axes follow wheel direction; Mac deltas follow content.
                    if phase.is_none() {
                        let x = wheel_steps(dx, &mut self.wheel.0);
                        let y = wheel_steps(dy, &mut self.wheel.1);
                        if x != 0 {
                            self.session.discrete(1, x)?;
                        }
                        if y != 0 {
                            self.session.discrete(0, y)?;
                        }
                    } else {
                        self.session
                            .axis(-dx as f64, -dy as f64, scroll_finished(phase))?;
                    }
                }
            }
            InputEvent::Key {
                scancode,
                pressed,
                repeat,
            } => {
                if !repeat {
                    if let Some(code) = evdev_from_mac(scancode, self.command_as_ctrl) {
                        self.key(code as i32, pressed)?;
                    }
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
                if !scale_delta.is_finite() {
                    return Ok(());
                }
                self.pinch += scale_delta;
                let steps = (self.pinch / 0.125).trunc() as i32;
                if steps != 0 {
                    self.pinch -= steps as f32 * 0.125;
                    let held = self.keys.contains(&29);
                    if !held {
                        self.key(29, true)?;
                    }
                    let result = self.session.discrete(0, steps.saturating_neg());
                    if !held {
                        self.key(29, false)?;
                    }
                    result?;
                }
            }
            InputEvent::Gesture { .. } => {}
        }
        Ok(())
    }
    fn release_all(&mut self) {
        for code in std::mem::take(&mut self.keys) {
            let _ = self.session.key(code, false);
        }
        for code in std::mem::take(&mut self.buttons) {
            let _ = self.session.button(code, false);
        }
        let _ = self.session.axis(0.0, 0.0, true);
        self.pinch = 0.0;
        self.wheel = (0.0, 0.0);
    }
}
impl Drop for PortalInjector {
    fn drop(&mut self) {
        self.release_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wheel_direction_and_gesture_end() {
        let mut rest = 0.0;
        assert_eq!(wheel_steps(16.0, &mut rest), 0);
        assert_eq!(wheel_steps(16.0, &mut rest), -1);
        assert_eq!(wheel_steps(-64.0, &mut rest), 2);
        assert!(scroll_finished(Some(GesturePhase::End)));
        assert!(scroll_finished(Some(GesturePhase::Cancel)));
        assert!(!scroll_finished(Some(GesturePhase::Update)));
    }
    #[test]
    fn buttons_are_evdev_not_x11() {
        assert_eq!(
            [
                MouseButton::Left,
                MouseButton::Right,
                MouseButton::Middle,
                MouseButton::X1,
                MouseButton::X2
            ]
            .map(button_code),
            [0x110, 0x111, 0x112, 0x113, 0x114]
        );
    }
    #[test]
    fn pointer_uses_portal_size_and_clamps() {
        assert_eq!(
            coordinates(0.5 * 1920.0, 0.5 * 1080.0, (1920, 1080)),
            (960.0, 540.0)
        );
        assert_eq!(coordinates(1920.0, -20.0, (1920, 1080)), (1919.0, 0.0));
        assert_eq!(
            coordinates(f64::NAN, f64::INFINITY, (1920, 1080)),
            (0.0, 0.0)
        );
    }
}
