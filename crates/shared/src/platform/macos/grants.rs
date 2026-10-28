//! The privacy grants: which are missing, where to give them, and the
//! system prompts that put the app into the lists.

use objc2_application_services::{
    AXIsProcessTrusted, AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt,
};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFString};
use objc2_core_graphics::{
    CGPreflightListenEventAccess, CGPreflightPostEventAccess, CGPreflightScreenCaptureAccess,
    CGRequestListenEventAccess,
};

const INPUT_MONITORING: &str = "Input Monitoring";
const ACCESSIBILITY: &str = "Accessibility";
const SCREEN_RECORDING: &str = "Screen Recording";

/// What `missing_permissions` can report, for `--check`. The microphone is
/// audio.rs's to report: macOS answers a denied mic with silence.
pub const PERMISSIONS: &[&str] = &[INPUT_MONITORING, ACCESSIBILITY, SCREEN_RECORDING];

/// The System Settings pane that grants `name`.
pub fn permission_help(name: &str) -> Option<&'static str> {
    Some(match name {
        INPUT_MONITORING => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
        ACCESSIBILITY => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        SCREEN_RECORDING => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
        _ => return None,
    })
}

/// Typing needs EITHER service: Accessibility or PostEvent (separate in TCC).
pub(super) fn may_post() -> bool {
    // SAFETY: a plain query without arguments.
    let ax = unsafe { AXIsProcessTrusted() };
    ax || CGPreflightPostEventAccess()
}

/// Only a bundle has a TCC identity that survives a rebuild; a dev binary
/// run from a terminal inherits the terminal's grants, so asking is noise.
fn in_bundle() -> bool {
    std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"))
}

/// Put the app into the Privacy lists and show the system prompts, once.
pub(super) fn request_permissions() {
    if !in_bundle() {
        return;
    }
    if !CGPreflightListenEventAccess() {
        CGRequestListenEventAccess();
    }
    if !may_post() {
        let opts = CFDictionary::<CFString, CFBoolean>::from_slices(
            // SAFETY: reading an immutable framework constant.
            &[unsafe { kAXTrustedCheckOptionPrompt }],
            &[CFBoolean::new(true)],
        );
        // SAFETY: the dictionary holds the documented key and a CFBoolean.
        unsafe { AXIsProcessTrustedWithOptions(Some(opts.as_opaque())) };
    }
}

pub(super) fn missing() -> Vec<&'static str> {
    let granted = [
        CGPreflightListenEventAccess(),
        may_post(),
        CGPreflightScreenCaptureAccess(),
    ];
    PERMISSIONS
        .iter()
        .zip(granted)
        .filter(|(_, ok)| !ok)
        .map(|(name, _)| *name)
        .collect()
}
