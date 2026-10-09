//! Sharing a Wayland desktop through the desktop portal (RemoteDesktop with
//! ScreenCast): frames through PipeWire, input through the same session.
//! See research/10-gnome-wayland.md.

#![cfg(target_os = "linux")]

pub mod input;
mod pipewire;
mod pod;
pub mod session;
pub mod source;

pub use input::PortalInjector;
pub use source::PortalSource;

/// DISPLAY can point at Xwayland, so only the session type selects Wayland.
pub fn use_portal() -> bool {
    choose_portal(
        std::env::var("SUNNA_CAPTURE").ok().as_deref(),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
    )
}
fn choose_portal(capture: Option<&str>, session: Option<&str>) -> bool {
    match capture {
        Some("x11") => false,
        Some("portal") => true,
        _ => session == Some("wayland"),
    }
}

pub fn authorize() -> anyhow::Result<()> {
    println!("In the dialog: switch on Allow Remote Interaction, keep Remember This Selection on, choose the screen, click Share.");
    let session = session::Session::start(std::time::Duration::from_secs(120), false)?;
    println!(
        "Sharing approved. Keyboard: {}; pointer: {}.",
        if session.devices & 1 != 0 {
            "granted"
        } else {
            "not granted"
        },
        if session.devices & 2 != 0 {
            "granted"
        } else {
            "not granted"
        }
    );
    if session.devices & 3 != 3 {
        println!("Remote interaction wasn't allowed: viewers will see the screen but can't use its mouse and keyboard. To allow it, run sunna-host setup --desktop again and switch on Allow Remote Interaction.");
    }
    if !session.remembered {
        println!("The desktop did not save permission; later connections may ask again.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_selection() {
        assert!(choose_portal(None, Some("wayland")));
        assert!(!choose_portal(None, Some("x11")));
        assert!(!choose_portal(Some("x11"), Some("wayland")));
        assert!(choose_portal(Some("portal"), Some("x11")));
        assert!(!choose_portal(None, None));
    }
}
