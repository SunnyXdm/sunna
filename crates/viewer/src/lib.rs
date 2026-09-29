//! The Sunna viewer: a native window showing a host's stream, with
//! keyboard capture, viewer hotkeys, the stats bar and notices. Used by
//! `sunna-cli view` and the Sunna app, which runs each session in its own
//! viewer process.

#[cfg(target_os = "linux")]
mod cursor;
#[cfg(target_os = "linux")]
mod gpu;
mod keyboard;
#[cfg(target_os = "linux")]
mod linux_keyboard;
mod keymap;
#[cfg(target_os = "macos")]
mod layer_presenter;
#[cfg(target_os = "macos")]
mod mac_keyboard;
#[cfg(target_os = "macos")]
mod menu;
mod menu_model;
#[cfg(target_os = "macos")]
mod remote_cursor;
mod send_keys;
mod session;
mod viewer;

pub use session::{run, ViewerArgs};
