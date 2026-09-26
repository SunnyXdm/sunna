//! The Sunna viewer: a native window showing a host's stream, with
//! keyboard capture, viewer hotkeys, the stats bar and notices. Used by
//! `sunna-cli view` and the Sunna app, which runs each session in its own
//! viewer process.

mod keyboard;
mod keymap;
#[cfg(target_os = "macos")]
mod layer_presenter;
#[cfg(target_os = "macos")]
mod mac_keyboard;
mod session;
mod viewer;

pub use session::{run, ViewerArgs};
