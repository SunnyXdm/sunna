//! Input: host-side injection and client-side capture traits.
//!
//! Platform injection backends (research/03 §6): Windows `SendInput` with
//! scancodes from a SYSTEM-context worker; macOS `CGEventPost` (Accessibility
//! TCC) including scroll phases/momentum for native-feeling trackpad scroll;
//! Linux `uinput` (also the virtual-gamepad and virtual-touchpad path,
//! research/05 §5). Gesture replay strategy per research/05 §4:
//! high-fidelity scroll, shortcut replay for system actions.

use sunna_proto::messages::InputEvent;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;

/// Host side: apply a remote input event to the local OS.
pub trait InputInjector: Send {
    fn inject(&mut self, event: &InputEvent) -> anyhow::Result<()>;
    /// Release every key and button this injector is holding down. Called
    /// when a session ends so a dropped connection can't leave, say, Cmd
    /// stuck on the host.
    fn release_all(&mut self) {}
}

/// The best injector this platform offers (log-only where none exists yet).
pub fn make_injector() -> Box<dyn InputInjector> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacInjector::new())
    }
    #[cfg(target_os = "linux")]
    {
        match linux::X11Injector::new() {
            Ok(injector) => Box::new(injector),
            Err(error) => {
                tracing::warn!("{error:#}; input will be logged, not injected");
                Box::new(LogInjector)
            }
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Box::new(LogInjector)
    }
}

/// Stand-in where input can't be injected: counts events so the input path
/// is visible in the logs. Never logs what they are: key events are what
/// someone typed (passwords included), and logs are shipped.
#[derive(Default)]
pub struct LogInjector;

impl InputInjector for LogInjector {
    fn inject(&mut self, _event: &InputEvent) -> anyhow::Result<()> {
        static RECEIVED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let received = RECEIVED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if received.is_power_of_two() {
            tracing::info!(received, "input events received (not injected: stub)");
        }
        Ok(())
    }
}
