//! The hotkey listener (and the clicks that end a revert offer): a listen-only CGEventTap on its own CFRunLoop thread.
//! Listen-only needs Input Monitoring alone and can never freeze the keyboard.

use super::MARK;
use blitznir_core::Result;
use blitznir_core::platform::{Key, KeyEvent};
use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType, CGPreflightListenEventAccess,
};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

/// Keycode, key, and its device-specific flag bit (NX_DEVICE*KEYMASK). The
/// generic flags cannot tell left from right, so with them a held left ⌥
/// would make a right-⌥ release look like a press.
const MODIFIERS: [(i64, Key, u64); 9] = [
    (56, Key::LeftShift, 0x02),
    (60, Key::RightShift, 0x04),
    (59, Key::LeftCtrl, 0x01),
    (62, Key::RightCtrl, 0x2000),
    (58, Key::LeftAlt, 0x20),
    (61, Key::RightAlt, 0x40),
    (55, Key::LeftMeta, 0x08),
    (54, Key::RightMeta, 0x10),
    (63, Key::Fn, 0x80_0000), // kCGEventFlagMaskSecondaryFn: fn has no device bit
];

/// Virtual keycodes of F1–F20 (macOS has no F21–F24).
const F_KEYS: [i64; 20] = [
    122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79, 80, 90,
];

/// The 3, 4 and 5 keys: with ⌘⇧ the system's screenshot shortcuts.
const SCREENSHOT_KEYS: [i64; 3] = [20, 21, 23];

/// How often a missing grant or a failed tap is retried.
const RETRY: Duration = Duration::from_secs(3);

struct Tap {
    /// The installed tap's port; replaced whenever the loop is set up again.
    port: Mutex<Option<CFRetained<CFMachPort>>>,
    tx: Sender<KeyEvent>,
}

impl Tap {
    fn port(&self) -> MutexGuard<'_, Option<CFRetained<CFMachPort>>> {
        blitznir_core::lock(&self.port)
    }
}

fn translate(etype: CGEventType, ev: &CGEvent) -> Option<KeyEvent> {
    let code = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode);
    if etype == CGEventType::FlagsChanged {
        let Some(&(_, key, bit)) = MODIFIERS.iter().find(|(c, _, _)| *c == code) else {
            return Some(KeyEvent::Other); // caps lock and friends
        };
        let down = CGEvent::flags(Some(ev)).bits() & bit != 0;
        return Some(KeyEvent::Modifier { key, down });
    }
    if etype == CGEventType::LeftMouseDown || etype == CGEventType::RightMouseDown {
        return Some(KeyEvent::Click);
    }
    if etype != CGEventType::KeyDown {
        return None;
    }
    let repeat = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventAutorepeat);
    let flags = CGEvent::flags(Some(ev));
    let cmd_shift = CGEventFlags::MaskCommand | CGEventFlags::MaskShift;
    if flags.contains(cmd_shift) && SCREENSHOT_KEYS.contains(&code) {
        return Some(KeyEvent::Screenshot);
    }
    match F_KEYS.iter().position(|c| *c == code) {
        Some(_) if repeat != 0 => None,
        Some(i) => Some(KeyEvent::FKey(i as u8 + 1)),
        None => Some(KeyEvent::Other),
    }
}

unsafe extern "C-unwind" fn callback(
    _proxy: CGEventTapProxy,
    etype: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    // SAFETY: user_info is the leaked Tap, touched only on the tap thread.
    let tap = unsafe { &*(user_info as *const Tap) };
    if etype == CGEventType::TapDisabledByTimeout || etype == CGEventType::TapDisabledByUserInput {
        if let Some(port) = tap.port().as_ref() {
            CGEvent::tap_enable(port, true);
        }
        // events were lost while disabled; a stale "held" key would eat a press
        let _ = tap.tx.send(KeyEvent::Reset);
        return event.as_ptr();
    }
    // SAFETY: the system hands us a valid event for the duration of the call.
    let ev = unsafe { event.as_ref() };
    if CGEvent::integer_value_field(Some(ev), CGEventField::EventSourceUserData) == MARK {
        return event.as_ptr(); // our own typing and shortcuts
    }
    if let Some(msg) = translate(etype, ev) {
        let _ = tap.tx.send(msg);
    }
    event.as_ptr()
}

/// Install the tap on this thread and run its loop; returns only on failure.
fn run(tap: &'static Tap) {
    let mask = [
        CGEventType::FlagsChanged,
        CGEventType::KeyDown,
        CGEventType::LeftMouseDown,
        CGEventType::RightMouseDown,
    ]
    .iter()
    .fold(0u64, |m, t| m | 1 << t.0);
    // SAFETY: callback matches CGEventTapCallBack; user_info outlives the tap (leaked).
    let port = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            mask,
            Some(callback),
            tap as *const Tap as *mut c_void,
        )
    };
    let Some(port) = port else {
        log::error!("CGEventTapCreate failed");
        return;
    };
    let Some(src) = CFMachPort::new_run_loop_source(None, Some(&port), 0) else {
        log::error!("no run loop source for the event tap");
        return;
    };
    let Some(rl) = CFRunLoop::current() else {
        log::error!("the event tap thread has no run loop");
        return;
    };
    // SAFETY: reading an immutable CoreFoundation constant.
    rl.add_source(Some(&src), unsafe { kCFRunLoopCommonModes });
    CGEvent::tap_enable(&port, true);
    *tap.port() = Some(port);
    log::info!("event tap installed: hotkeys active");
    CFRunLoop::run();
    *tap.port() = None; // dead with its loop
}

/// The tap thread. A port is not proof that events arrive: without Input
/// Monitoring it stays silent, so wait for the grant first.
pub fn spawn(tx: Sender<KeyEvent>) -> Result<()> {
    thread::Builder::new()
        .name("event-tap".into())
        .spawn(move || {
            let mut asked = false;
            while !CGPreflightListenEventAccess() {
                if !asked {
                    log::warn!(
                        "hotkeys wait for Input Monitoring (System Settings → Privacy & Security)"
                    );
                    asked = true;
                }
                thread::sleep(RETRY);
            }
            let tap: &'static Tap = Box::leak(Box::new(Tap {
                port: Mutex::new(None),
                tx,
            }));
            loop {
                run(tap);
                thread::sleep(RETRY);
            }
        })?;
    Ok(())
}
