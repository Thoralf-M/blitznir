//! The app's non-UI half (package `blitznir-shared`): the OS behind core's
//! `Platform` seam, the microphone and the speaker, the speech model, one dictation session
//! per hotkey press, the log and the CLI. No UI toolkit: crates/app
//! builds on it.
#![deny(unsafe_code)]

pub mod audio;
pub mod cli;
pub mod hands;
pub mod logging;
#[allow(unsafe_code)]
pub mod platform;
pub mod session;
pub mod speaker;
pub mod stt;
pub mod wake;
