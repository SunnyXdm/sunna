//! The installed macOS app links the screen host into its signed executable.
#[cfg(target_os = "macos")]
mod about;
#[cfg(target_os = "macos")]
pub mod macos;
