//! The seams to the OS and to the speech model. `crates/app` implements
//! [`Platform`] per OS and [`Recognizer`] with Parakeet; [`fake`] implements
//! both for tests.

use crate::{Result, config::Config};
use std::{path::Path, sync::mpsc::Sender};

pub use crate::keys::{Key, KeyEvent};

pub mod fake;

/// The app that had focus before our window showed: a pid on mac, None on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Focus(pub Option<i32>);

/// Proof of a copy request: the pasteboard changeCount on mac.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CopyToken(pub i64);

impl CopyToken {
    /// No copy keys were sent: there is no copy to wait for.
    pub const NONE: CopyToken = CopyToken(-1);

    pub fn sent(self) -> bool {
        self.0 >= 0
    }
}

/// A rectangle on screen in points, not pixels, in the one desktop space
/// that macOS Accessibility, CoreGraphics and tao's logical monitor, window
/// and cursor positions share: origin at the top left of the main screen
/// (the one with the menu bar), y grows down, the other screens at their
/// arranged offsets (negative left of or above it). No screen id is needed:
/// the rect says which screen it is on.
/// Where a selection is on screen: the selected text itself (`text`), or
/// only the field or window that holds it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub rect: Rect,
    pub text: bool,
}

/// Area per selected character no real text needs: a 48 pt heading's is
/// under 2,000 pt². Some apps answer a word's range with the whole column
/// it sits in (70 × 822 pt for a word of 4 characters, measured).
const AREA_PER_CHAR: f64 = 3_000.0;

impl Bounds {
    /// False: said to be the text, but far too big for `selected`, so it
    /// says nothing about where the text is.
    pub fn fits(&self, selected: &str) -> bool {
        // no text (the copy came back empty): nothing to weigh the rect by
        let n = selected.trim().chars().count() as f64;
        !self.text || n == 0.0 || self.rect.w * self.rect.h <= n * AREA_PER_CHAR
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn contains(&self, (x, y): (f64, f64)) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    pub fn centre(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }

    /// An app that does not know answers with a zero rect or one far off
    /// every screen; anything else is taken at its word.
    pub fn plausible(&self) -> bool {
        self.w > 2.0 && self.h > 2.0 && self.x > -20_000.0 && self.y > -20_000.0
    }
}

pub trait Platform: Send + Sync {
    /// Run the key listener on its own thread. Never swallows, skips our own events.
    fn listen(&self, tx: Sender<KeyEvent>) -> Result<()>;
    fn type_text(&self, s: &str) -> Result<()>;
    fn backspace(&self, n: usize) -> Result<()>;
    fn wait_modifiers_released(&self);
    /// Call before our window shows.
    fn focused_app(&self) -> Focus;
    /// mac: AX role + AXValue settable; Linux: None (unknown).
    fn focused_editable(&self, f: &Focus) -> Option<bool>;
    /// mac: send ⌘C before we take focus; Linux: no-op.
    fn begin_copy(&self) -> CopyToken;
    /// mac: wait for the changeCount; Linux: read PRIMARY once we have focus.
    fn finish_copy(&self, t: CopyToken) -> Option<String>;
    /// Where `f`'s selection is on screen: the selected text, else its
    /// focused element, else its focused window; always [`Rect::plausible`].
    /// mac: Accessibility, which answers for a background app too, so ask
    /// after the copy. Linux: None (no compositor says where anything is).
    fn selection_bounds(&self, f: &Focus) -> Option<Bounds>;
    /// `f`'s selected text, None if blank or only a caret, read without ⌘C
    /// (the clipboard stays): after each mouse selection, for `[translate]
    /// on_select`, so it never logs. mac: Accessibility; Linux: None (no
    /// compositor says where a button by the text would go).
    fn selected_text(&self, f: &Focus) -> Option<String>;
    /// The line `f`'s caret is on, from the line's start to the caret
    /// (`x + w`; `w` 0: the start is unknown), `y` and `h` its line box; None:
    /// no caret, or the app will not say. Asked a few times a second while a
    /// dictation records (the wave under it), so it never logs. mac:
    /// Accessibility; Linux: None, as [`Platform::selection_bounds`].
    fn caret(&self, f: &Focus) -> Option<Rect>;
    /// `f`'s focused text from its start to the caret; None while text is
    /// selected or the app will not say: hands-free's take-back phrase
    /// removes its last sentence. mac: Accessibility; Linux: None (it takes
    /// back only what its last dictation typed).
    fn text_before_caret(&self, f: &Focus) -> Option<String>;
    /// Where the mouse is, in [`Rect`]'s space: lightning aims there when
    /// the app will not say where its selection is. Linux: None (Wayland
    /// tells no client where the pointer is outside its own windows).
    fn mouse(&self) -> Option<(f64, f64)>;
    fn set_clipboard(&self, s: &str) -> Result<()>;
    /// The image on the clipboard as PNG bytes, for the panel's image paste;
    /// None: there is none.
    fn clipboard_image(&self) -> Option<Vec<u8>>;
    /// `png` on the clipboard as an image: Copy on a chat's image.
    fn set_clipboard_image(&self, png: &[u8]) -> Result<()>;
    /// Re-activate `f` and paste; false = left on the clipboard.
    fn paste_into(&self, f: &Focus) -> Result<bool>;
    /// Re-activate `f` as `paste_into` does, and nothing else: a job without
    /// the panel ended unpasted while the effect's overlay had the key focus.
    /// False = `f` never came back. Linux: a no-op (the compositor hands the
    /// focus back when the overlay goes; Wayland lets no client take it).
    fn refocus(&self, f: &Focus) -> bool;
    /// Show our hidden windows again without taking the focus from the app
    /// in front, which keeps the keyboard (macOS; a no-op elsewhere).
    fn show_quietly(&self);
    /// config.toml was saved: what the platform read at the start takes
    /// `cfg` now (Linux: `[linux]`, the typing layout and keys; a no-op on
    /// macOS, which asks the system each time).
    fn configure(&self, cfg: &Config);
    /// The desktop's light or dark preference where the window system does
    /// not tell (Wayland); None: none known (macOS: always, AppKit tells).
    fn prefers_dark(&self) -> Option<bool>;
    /// Re-activate `f` as `paste_into` does and delete the `n` characters
    /// before its cursor (a live dictation's, before its rewrite is pasted),
    /// at once; false = `f` never came back. Linux: where the focus is.
    /// Backspace, never a selection the paste replaces: macOS's
    /// platform/macos/backspace.rs says why.
    fn backspace_into(&self, f: &Focus, n: usize) -> Result<bool>;
    /// Re-activate `f` as `paste_into` does and send its undo shortcut
    /// (⌘Z; Linux: `[linux] undo_keys`); false = `f` never came back.
    fn undo(&self, f: &Focus) -> Result<bool>;
    /// Re-activate `f` as `paste_into` does and press Return: `[wake] send`
    /// sends what was dictated there. False = `f` never came back. Linux:
    /// where the focus is, as `undo`.
    fn send(&self, f: &Focus) -> Result<bool>;
    /// Run the region screenshot `cmd` with `{path}` replaced by `out`,
    /// shell-quoted (the data dir has a space on mac); false = the user cancelled.
    fn capture(&self, cmd: &str, out: &Path) -> Result<bool>;
    fn missing_permissions(&self) -> Vec<&'static str>;
    /// Hand `url` to the desktop: a link in an answer, the release page, or
    /// (macOS) the System Settings pane of a missing grant.
    fn open_url(&self, url: &str) -> Result<()>;
    /// Play the WAV file `path` once, in the background (the thunder); the
    /// player's process, to stop it early and to reap. mac: afplay; Linux:
    /// pw-play, PipeWire's player, on GNOME; the Fake plays nothing.
    fn play_sound(&self, path: &Path) -> Result<Option<std::process::Child>>;
}

/// One recognizer token. A token that starts a word has a leading space
/// (SentencePiece ▁); `end_s` counts from the start of the decoded buffer.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub text: String,
    pub end_s: f32,
}

impl Token {
    pub fn new(text: &str, end_s: f32) -> Token {
        Token {
            text: text.into(),
            end_s,
        }
    }
}

pub trait Recognizer: Send {
    /// Decode 16 kHz mono audio in one pass.
    fn transcribe(&mut self, pcm16k: &[f32]) -> Result<Vec<Token>>;
}

/// `s` as one POSIX shell word: `[capture] cmd` gets its `{path}` this way.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_survives_spaces_and_quotes() {
        assert_eq!(shell_quote("/a b/c"), "'/a b/c'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn a_zero_or_far_off_rect_is_an_app_saying_it_does_not_know() {
        let line = Rect {
            x: 120.0,
            y: 300.0,
            w: 410.0,
            h: 17.0,
        };
        assert!(line.plausible());
        let left_screen = Rect { x: -1800.0, ..line };
        assert!(left_screen.plausible(), "a screen left of the main one");
        for dunno in [
            Rect::default(),
            Rect { w: 0.0, ..line },
            Rect { h: 1.0, ..line },
            Rect {
                y: -32_000.0,
                ..line
            },
        ] {
            assert!(!dunno.plausible(), "{dunno:?}");
        }
        assert!(line.contains((120.0, 300.0)) && !line.contains((530.0, 310.0)));
        assert_eq!(line.centre(), (325.0, 308.5));
        assert!(line.intersects(&Rect { x: 500.0, ..line }));
        assert!(
            !line.intersects(&Rect { x: 530.0, ..line }),
            "edges only touch"
        );
    }

    #[test]
    fn a_text_rect_far_too_big_for_its_text_is_not_believed() {
        let at = |w, h, text| Bounds {
            rect: Rect {
                x: 0.0,
                y: 160.0,
                w,
                h,
            },
            text,
        };
        assert!(
            !at(70.0, 822.0, true).fits("More"),
            "a whole column for one word"
        );
        assert!(
            at(70.0, 822.0, false).fits("More"),
            "a field is only a field"
        );
        assert!(at(131.0, 16.0, true).fits("a short line of 23 chs"));
        assert!(at(40.0, 60.0, true).fits("H"), "one big heading letter");
        let para = "word ".repeat(80); // wrapped into many lines
        assert!(at(600.0, 160.0, true).fits(&para));
    }

    #[test]
    fn the_fake_says_where_the_selection_is_only_when_told() {
        let mut p = fake::FakePlatform::new();
        let f = p.focused_app();
        assert_eq!(p.selection_bounds(&f), None);
        let r = Rect {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 16.0,
        };
        let b = Bounds {
            rect: r,
            text: true,
        };
        p.bounds = Some(b);
        assert_eq!(p.selection_bounds(&f), Some(b));
    }

    #[test]
    fn the_fakes_keys_reach_a_detector_as_an_os_listeners_do() {
        use crate::keys::{Action, Detector};
        use std::time::{Duration, Instant};
        let p = fake::FakePlatform::new();
        let alt = |down| KeyEvent::Modifier {
            key: Key::RightAlt,
            down,
        };
        let taps = [
            alt(true),
            alt(false),
            alt(true),
            alt(false),
            KeyEvent::FKey(13),
        ];
        p.keys.lock().unwrap().extend(taps);
        let (tx, rx) = std::sync::mpsc::channel();
        p.listen(tx).unwrap();
        let bindings = [(Key::RightAlt, Action::Live), (Key::F(13), Action::Chat)];
        let mut d = Detector::new(bindings.into(), Duration::from_millis(450));
        let t0 = Instant::now();
        let fired: Vec<Action> = (0u64..)
            .zip(rx.try_iter())
            .filter_map(|(i, ev)| d.feed(ev, t0 + Duration::from_millis(50 * i)))
            .collect();
        assert_eq!(fired, [Action::Live, Action::Chat]);
        assert!(p.keys.lock().unwrap().is_empty(), "each key is sent once");
    }
}
