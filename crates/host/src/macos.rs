//! Only the embedded screen host opts into these hooks. Standalone
//! synthetic hosts and their tests do not require desktop permissions.
use std::sync::OnceLock;

pub struct DesktopEvents {
    pub viewer_changed: fn(Option<&str>),
    pub capture_failed: fn(),
    pub unavailable: fn() -> Option<&'static str>,
}

static DESKTOP: OnceLock<DesktopEvents> = OnceLock::new();

pub fn register(events: DesktopEvents) {
    DESKTOP
        .set(events)
        .ok()
        .expect("one embedded host per process");
}

pub(super) fn viewer_changed(viewer: Option<&str>) {
    if let Some(events) = DESKTOP.get() {
        (events.viewer_changed)(viewer);
    }
}

pub(super) fn unavailable() -> Option<&'static str> {
    DESKTOP.get().and_then(|events| (events.unavailable)())
}

pub(super) fn session_guard() -> anyhow::Result<Option<std::fs::File>> {
    if DESKTOP.get().is_some() {
        let app = sunna_update::Updater::new("app")?;
        let guard = app.lock("session.lock", true)?;
        if std::env::var_os("SUNNA_RECOVERY_ONLY").is_none()
            && std::env::current_exe()?
                .canonicalize()?
                .starts_with(app.root.join("versions"))
        {
            anyhow::ensure!(app.is_managed_process(), "The selected host changed.");
        }
        return Ok(Some(guard));
    }
    sunna_update::session_guard()
}

pub fn capture_failed() {
    if let Some(events) = DESKTOP.get() {
        (events.capture_failed)();
    }
}
