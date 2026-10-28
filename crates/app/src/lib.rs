//! The app's code, a library so its UI tests (tests/ui.rs) can build the
//! panel too; main.rs is the binary.
#![deny(unsafe_code)]

mod autostart;
pub mod daemon;
pub mod panel;
pub mod tray;
pub mod ui;
#[cfg(target_os = "linux")]
mod wayland;
mod zap;

/// Every window's Wayland app id and X11 class: GNOME finds the dock icon in
/// the .desktop file of this name (scripts/install-linux.sh).
const APP_ID: &str = "blitznir";
