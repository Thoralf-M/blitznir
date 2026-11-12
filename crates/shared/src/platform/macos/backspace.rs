//! Taking a live dictation back before its rewrite is pasted: Backspace,
//! in every app. Why not a selection the paste replaces (one edit, one ⌘Z):
//! an Accessibility selection is read back as set by Electron, which only
//! moves the caret, so the paste landed in front of the dictation; Shift+Left
//! moves a terminal's cursor or types an escape sequence there, and telling
//! a terminal apart takes a list of apps. Backspace works everywhere, as
//! fast in one burst; the cost is a second ⌘Z to get the dictation back
//! (the revert box does it in one).

use super::{KEY_DELETE, front, post_marked, wait_released};
use blitznir_core::platform::Focus;
use objc2_core_graphics::{CGEvent, CGEventFlags};
use std::{thread, time::Duration};

/// Let the app take the keys before the paste.
const SETTLE: Duration = Duration::from_millis(20);
/// A dictation hotkey can still be held when its rewrite lands: with ⌥ down,
/// Backspace deletes words, so the take-back waits for every modifier to be
/// let go, longer than a keystroke's 2 s.
const RELEASE: Duration = Duration::from_secs(10);

/// Every modifier let go, or [`RELEASE`] over (then logged: the keys may
/// come out as shortcuts).
fn released() {
    if !wait_released(RELEASE) {
        log::warn!("a modifier is still held after {RELEASE:?}: taking the dictation back anyway");
    }
}

/// `f` brought back and `n` Backspaces in one burst, not one every 2 ms:
/// gone at once, as far as the eye can tell (a key every 2 ms took a
/// 500-character dictation over a second).
pub fn backspace(f: &Focus, n: usize) -> bool {
    released();
    if !front(f) {
        return false;
    }
    for _ in 0..n {
        for down in [true, false] {
            if let Some(ev) = CGEvent::new_keyboard_event(None, KEY_DELETE, down) {
                post_marked(&ev, CGEventFlags::empty());
            }
        }
    }
    thread::sleep(SETTLE);
    true
}
