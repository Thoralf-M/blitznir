//! macOS: CoreGraphics events, NSPasteboard, Accessibility.

mod backspace;
mod caret;
mod focus;
mod grants;
mod tap;
mod window;

use blitznir_core::platform::{Bounds, CopyToken, Focus, KeyEvent, Platform, Rect};
use blitznir_core::{Error, Result};
use focus::{bring_front, editable, frontmost, nothing_selected, selection_bounds, yield_to};
pub use grants::{PERMISSIONS, permission_help};
use grants::{may_post, request_permissions};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardTypePNG,
    NSPasteboardTypeString, NSPasteboardTypeTIFF,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventType,
};
use objc2_foundation::{NSData, NSDictionary, NSString};
use std::path::Path;
use std::process::{Child, Command};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};
pub use window::{no_title_bar_zoom, set_frame, visible_frame};

/// kCGEventSourceUserData on every event we post, so our own tap skips them.
const MARK: i64 = 0x424C_5A52; // "BLZR"
const KEY_C: u16 = 8;
const KEY_V: u16 = 9;
/// Where US keyboards have Z; [`layout_key`] finds it on the one in use.
const KEY_Z: u16 = 6;
const KEY_DELETE: u16 = 51;
const KEY_LEFT_CMD: u16 = 55;
/// NX_DEVICELCMDKEYMASK: a real ⌘ press sets it next to the generic flag.
const DEVICE_LEFT_CMD: u64 = 0x08;
pub struct Mac;

fn post_marked(ev: &CGEvent, flags: CGEventFlags) {
    CGEvent::set_flags(Some(ev), flags);
    CGEvent::set_integer_value_field(Some(ev), CGEventField::EventSourceUserData, MARK);
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(ev));
}

fn key_press(code: u16) {
    for down in [true, false] {
        if let Some(ev) = CGEvent::new_keyboard_event(None, code, down) {
            post_marked(&ev, CGEventFlags::empty());
        }
    }
    thread::sleep(Duration::from_millis(2));
}

/// ⌘+key.
fn cmd_key(key: u16) {
    let cmd = CGEventFlags::MaskCommand | CGEventFlags::from_bits_retain(DEVICE_LEFT_CMD);
    held(KEY_LEFT_CMD, cmd, key, 1);
}

/// `key` `times` over with the modifier `modifier` held, sent like a real
/// keyboard: its own flagsChanged down, the keys with its `flags`, its up.
/// Apps (Electron's among them) take the modifier from those, not from the
/// key's flags alone; without the up the session state shows it held for
/// > 2 s and the next wait for released modifiers stalls.
fn held(modifier: u16, flags: CGEventFlags, key: u16, times: usize) {
    wait_released(Duration::from_secs(2));
    let post = |code, down, flags, is_modifier| {
        if let Some(ev) = CGEvent::new_keyboard_event(None, code, down) {
            if is_modifier {
                CGEvent::set_type(Some(&ev), CGEventType::FlagsChanged);
            }
            post_marked(&ev, flags);
        }
    };
    post(modifier, true, flags, true);
    thread::sleep(Duration::from_millis(10));
    // one burst: a pause per key showed a long selection growing (2 ms × n)
    for _ in 0..times {
        post(key, true, flags, false);
        post(key, false, flags, false);
    }
    thread::sleep(Duration::from_millis(8));
    post(modifier, false, CGEventFlags::empty(), true);
    thread::sleep(Duration::from_millis(10));
}

/// The key that types `ch` on the current layout, else `ansi`: an app
/// matches ⌘Z by the character, and a German layout has Z on the US Y key.
/// The layout answers on the main thread only: elsewhere, an event has no
/// characters (measured on macOS 26).
fn layout_key(ch: char, ansi: u16) -> u16 {
    let types = move |code: u16| {
        let Some(ev) = CGEvent::new_keyboard_event(None, code, true) else {
            return false;
        };
        let (mut buf, mut len) = ([0u16; 4], 0);
        // SAFETY: `buf` has room for the 4 units asked for; `len` is a valid out-pointer.
        unsafe { CGEvent::keyboard_get_unicode_string(Some(&ev), 4, &mut len, buf.as_mut_ptr()) };
        String::from_utf16(&buf[..len as usize]).is_ok_and(|s| s.chars().eq([ch]))
    };
    // the letter keys are codes 0 to 50 on every Apple keyboard
    dispatch2::run_on_main(move |_| (0..=50).find(|&c| types(c))).unwrap_or(ansi)
}

/// Re-activate `f`; false: it never came to the front.
fn front(f: &Focus) -> bool {
    let Some(pid) = f.0 else {
        return false;
    };
    yield_to(pid);
    if !bring_front(pid) {
        log::warn!("pid {pid} never came back to the front");
        return false;
    }
    thread::sleep(Duration::from_millis(120)); // let its window become key
    true
}

/// Re-activate `f` and send ⌘`key` there; false: it never came to the front.
fn cmd_into(f: &Focus, key: impl FnOnce() -> u16) -> Result<bool> {
    if !front(f) {
        return Ok(false);
    }
    cmd_key(key());
    Ok(true)
}

/// Never type while the user holds a modifier: the app would see shortcuts.
fn wait_released(max: Duration) -> bool {
    let mods = CGEventFlags::MaskCommand
        | CGEventFlags::MaskAlternate
        | CGEventFlags::MaskControl
        | CGEventFlags::MaskShift;
    let end = Instant::now() + max;
    while CGEventSource::flags_state(CGEventSourceStateID::CombinedSessionState).intersects(mods) {
        if Instant::now() > end {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
    true
}

/// ≤ 16 UTF-16 units per event (the API's limit), never ending on a high surrogate.
fn utf16_chunks(s: &str) -> Vec<Vec<u16>> {
    let u: Vec<u16> = s.encode_utf16().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < u.len() {
        let mut end = (i + 16).min(u.len());
        if end < u.len() && (0xD800..=0xDBFF).contains(&u[end - 1]) {
            end -= 1;
        }
        out.push(u[i..end].to_vec());
        i = end;
    }
    out
}

/// NSPasteboard is main-thread only: from any other thread AppKit traps
/// ("mutated while enumerated") beside Freya's own clipboard reads. On the
/// main thread this runs inline.
fn on_pasteboard<T: Send>(f: impl FnOnce(&NSPasteboard) -> T + Send) -> T {
    dispatch2::run_on_main(move |_| f(&NSPasteboard::generalPasteboard()))
}

fn clipboard_text() -> Option<String> {
    on_pasteboard(|pb| {
        // SAFETY: reading an immutable AppKit constant.
        let kind = unsafe { NSPasteboardTypeString };
        pb.stringForType(kind).map(|s| s.to_string())
    })
}

impl Platform for Mac {
    fn listen(&self, tx: Sender<KeyEvent>) -> Result<()> {
        request_permissions();
        tap::spawn(tx)
    }

    fn type_text(&self, s: &str) -> Result<()> {
        if !may_post() {
            return Err(Error::Platform(
                "cannot type: grant Accessibility in System Settings → Privacy & Security".into(),
            ));
        }
        wait_released(Duration::from_secs(2));
        for chunk in utf16_chunks(s) {
            for down in [true, false] {
                let Some(ev) = CGEvent::new_keyboard_event(None, 0, down) else {
                    continue;
                };
                // SAFETY: pointer + length describe `chunk`, alive for the call.
                unsafe {
                    CGEvent::keyboard_set_unicode_string(
                        Some(&ev),
                        chunk.len() as _,
                        chunk.as_ptr(),
                    )
                };
                post_marked(&ev, CGEventFlags::empty());
            }
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    }

    fn backspace(&self, n: usize) -> Result<()> {
        for _ in 0..n {
            key_press(KEY_DELETE);
        }
        Ok(())
    }

    fn wait_modifiers_released(&self) {
        wait_released(Duration::from_secs(2));
    }

    fn focused_app(&self) -> Focus {
        Focus(frontmost())
    }

    fn focused_editable(&self, f: &Focus) -> Option<bool> {
        f.0.and_then(editable)
    }

    /// No other app in front (ours is, or none): no ⌘C, since it would land
    /// in our own window. That counts as nothing selected. Nor when the
    /// focused text field says its selection is empty: a ⌘C would change
    /// nothing, and finish_copy would wait its whole timeout to learn that.
    fn begin_copy(&self) -> CopyToken {
        let Some(pid) = frontmost() else {
            return CopyToken::NONE;
        };
        if nothing_selected(pid) {
            log::info!("nothing selected (the focused text field says so): no ⌘C");
            return CopyToken::NONE;
        }
        let before = on_pasteboard(|pb| pb.changeCount());
        cmd_key(KEY_C);
        CopyToken(before as i64)
    }

    /// The counter is the only honest signal: an unchanged one means nothing
    /// was selected, while the string would be whatever was copied before.
    fn finish_copy(&self, t: CopyToken) -> Option<String> {
        if !t.sent() {
            return None;
        }
        let end = Instant::now() + Duration::from_millis(1500);
        let count = || on_pasteboard(|pb| pb.changeCount()) as i64;
        while count() == t.0 && Instant::now() < end {
            thread::sleep(Duration::from_millis(30));
        }
        (count() != t.0).then(clipboard_text).flatten()
    }

    fn selection_bounds(&self, f: &Focus) -> Option<Bounds> {
        f.0.and_then(selection_bounds)
    }

    fn selected_text(&self, f: &Focus) -> Option<String> {
        f.0.and_then(focus::selected_text)
    }

    fn caret(&self, f: &Focus) -> Option<Rect> {
        f.0.and_then(caret::line)
    }

    /// A null event's location: global points from the main screen's top left.
    fn mouse(&self) -> Option<(f64, f64)> {
        CGEvent::new(None)
            .map(|ev| CGEvent::location(Some(&ev)))
            .map(|p| (p.x, p.y))
    }

    fn set_clipboard(&self, s: &str) -> Result<()> {
        let set = on_pasteboard(|pb| {
            pb.clearContents();
            // SAFETY: reading an immutable AppKit constant.
            let kind = unsafe { NSPasteboardTypeString };
            pb.setString_forType(&NSString::from_str(s), kind)
        });
        if set {
            Ok(())
        } else {
            Err(Error::Platform("the pasteboard refused the text".into()))
        }
    }

    /// PNG as it is; TIFF (what Preview and most browsers copy) re-encoded.
    fn clipboard_image(&self) -> Option<Vec<u8>> {
        on_pasteboard(|pb| {
            // SAFETY: reading immutable AppKit constants.
            let (png, tiff) = unsafe { (NSPasteboardTypePNG, NSPasteboardTypeTIFF) };
            if let Some(data) = pb.dataForType(png) {
                return Some(data.to_vec());
            }
            let tiff = pb.dataForType(tiff)?;
            let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
            // SAFETY: an empty properties dictionary is of every key and value type.
            let data = unsafe {
                rep.representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &NSDictionary::new(),
                )
            };
            data.map(|d| d.to_vec())
        })
    }

    fn set_clipboard_image(&self, png: &[u8]) -> Result<()> {
        let png = png.to_vec();
        let set = on_pasteboard(move |pb| {
            let data = NSData::with_bytes(&png);
            pb.clearContents();
            // SAFETY: reading an immutable AppKit constant.
            pb.setData_forType(Some(&data), unsafe { NSPasteboardTypePNG })
        });
        set.then_some(())
            .ok_or_else(|| Error::Platform("the pasteboard refused the image".into()))
    }

    fn paste_into(&self, f: &Focus) -> Result<bool> {
        cmd_into(f, || KEY_V)
    }

    fn refocus(&self, f: &Focus) -> bool {
        front(f)
    }

    fn undo(&self, f: &Focus) -> Result<bool> {
        cmd_into(f, || layout_key('z', KEY_Z))
    }

    fn show_quietly(&self) {
        focus::show_quietly();
    }

    /// Nothing read at the start: the layout is the system's, asked each time.
    fn configure(&self, _cfg: &blitznir_core::config::Config) {}

    /// AppKit tells the windows itself.
    fn prefers_dark(&self) -> Option<bool> {
        None
    }

    /// Backspace in every app, not a selection: backspace.rs says why.
    fn backspace_into(&self, f: &Focus, n: usize) -> Result<bool> {
        Ok(backspace::backspace(f, n))
    }

    fn capture(&self, cmd: &str, out: &Path) -> Result<bool> {
        super::run_capture(cmd, out)
    }

    fn open_url(&self, url: &str) -> Result<()> {
        super::spawn_reaped("/usr/bin/open", url)
    }

    fn play_sound(&self, path: &Path) -> Result<Option<Child>> {
        let child = Command::new("/usr/bin/afplay").arg(path).spawn()?;
        Ok(Some(child))
    }

    fn missing_permissions(&self) -> Vec<&'static str> {
        grants::missing()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_never_split_a_surrogate_pair() {
        let s = "Grüße, Straße! 𝄞 done";
        let c = utf16_chunks(s);
        assert_eq!(c[0].len(), 15, "𝄞 starts at unit 15");
        assert_eq!(String::from_utf16(&c.concat()).unwrap(), s);
        assert!(
            c.iter()
                .all(|x| x.len() <= 16 && String::from_utf16(x).is_ok())
        );
    }

    #[test]
    fn our_mark_round_trips() {
        let (ev, field) = (
            CGEvent::new_keyboard_event(None, 0, true).unwrap(),
            CGEventField::EventSourceUserData,
        );
        CGEvent::set_integer_value_field(Some(&ev), field, MARK);
        assert_eq!(CGEvent::integer_value_field(Some(&ev), field), MARK);
    }
}
