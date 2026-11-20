//! Monitor X11 without opening a capture pipeline while the machine is idle.
use std::time::Duration;
use sunna_host::DesktopState;
use tokio::sync::watch;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, ImageFormat};
use x11rb::protocol::xtest::ConnectionExt;

pub const INPUT: &str = "Its desktop doesn't support remote input.";
pub const CAPTURE: &str = "Sunna can't capture its X11 display.";
pub const COLOR: &str = "Its display needs 24-bit or 32-bit color.";

pub fn check(width: Option<u32>, height: Option<u32>) -> DesktopState {
    let (conn, index) = sunna_capture::x11::connect(None).map_err(|_| {
        if absent() {
            "Its desktop isn't running."
        } else {
            "Sunna can't open its X11 display."
        }
    })?;
    let screen = &conn.setup().roots[index];
    if screen.root_depth < 24 {
        return Err(COLOR);
    }
    conn.xtest_get_version(2, 2)
        .map_err(|_| INPUT)?
        .reply()
        .map_err(|_| INPUT)?;
    // Exercise the same core X request available to the capture fallback.
    let pixel = conn
        .get_image(ImageFormat::Z_PIXMAP, screen.root, 0, 0, 1, 1, u32::MAX)
        .map_err(|_| CAPTURE)?
        .reply()
        .map_err(|_| CAPTURE)?;
    if pixel.data.is_empty() {
        return Err(CAPTURE);
    }
    Ok((
        width.unwrap_or(screen.width_in_pixels.into()).max(2) & !1,
        height.unwrap_or(screen.height_in_pixels.into()).max(2) & !1,
    ))
}

pub fn monitor(
    mut check: impl FnMut() -> DesktopState + Send + 'static,
    initial: DesktopState,
) -> watch::Receiver<DesktopState> {
    let (tx, rx) = watch::channel(initial);
    std::thread::spawn(move || {
        let mut previous = initial;
        let mut delay = 1;
        while !tx.is_closed() {
            std::thread::sleep(Duration::from_secs(delay));
            let state = check();
            if state != previous {
                if tx.send(state).is_err() {
                    break;
                }
                previous = state;
            }
            delay = if state.is_err() {
                (delay * 2).min(5)
            } else {
                2
            };
        }
    });
    rx
}

/// An absent local socket is environmental. Authentication, XTEST, capture
/// and codec errors on a running server must still fail release probation.
pub fn absent() -> bool {
    use std::io::ErrorKind;
    let Ok(display) = std::env::var("DISPLAY") else {
        return true;
    };
    if display.is_empty() {
        return true;
    }
    let Some(local) = display.strip_prefix(':') else {
        return false;
    };
    let Some(number) = local.split('.').next().and_then(|n| n.parse::<u16>().ok()) else {
        return false;
    };
    std::os::unix::net::UnixStream::connect(format!("/tmp/.X11-unix/X{number}"))
        .is_err_and(|e| matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused))
}

/// Restore on the monitor thread, so a slow portal cannot delay binding
/// the listener. The grant must outlive the short-lived PipeWire check.
pub fn monitor_portal(size: (u32, u32)) -> watch::Receiver<DesktopState> {
    let mut grant: Option<std::sync::Arc<sunna_portal::session::Session>> = None;
    monitor(
        move || {
            if grant
                .as_ref()
                .is_some_and(|s| !s.closed.load(std::sync::atomic::Ordering::Acquire))
            {
                return Ok(size);
            }
            grant.take();
            if let (Ok(runtime), Ok(display)) = (
                std::env::var("XDG_RUNTIME_DIR"),
                std::env::var("WAYLAND_DISPLAY"),
            ) {
                if !std::path::Path::new(&runtime).join(display).exists() {
                    return Err("Its desktop isn't running.");
                }
            }
            if !sunna_portal::session::has_restore_token() && !sunna_portal::session::approved() {
                return Err(sunna_host::PORTAL_PERMISSION);
            }
            let session =
                sunna_portal::session::acquire().map_err(|_| sunna_host::PORTAL_PERMISSION)?;
            // PortalSource::new waits for a real PipeWire frame.
            let _frame =
                sunna_portal::PortalSource::new(Some(320), Some(240), 2).map_err(|_| CAPTURE)?;
            grant = Some(session);
            Ok(size)
        },
        Err(sunna_host::PORTAL_PERMISSION),
    )
}
