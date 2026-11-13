//! Linux (GNOME Wayland first): evdev hotkeys, a uinput keyboard, PRIMARY
//! through GTK. Below the compositor where possible, so the same code runs
//! on X11, Mutter, KWin and wlroots.

mod clip;
mod control;
mod keymap;
mod keys;
mod pipe;
mod portal;
mod scheme;
mod seat;
mod typing;

use blitznir_core::{
    Error, Result,
    config::{Config, Linux as LinuxConfig, Paths},
    lock,
    platform::{Bounds, CopyToken, Focus, KeyEvent, Platform, Rect},
};
use std::{
    path::Path,
    sync::{Mutex, mpsc::Sender},
    thread,
    time::{Duration, Instant},
};
use typing::{Keyboard, Run};

const INPUT_GROUP: &str = "input group";
const UINPUT: &str = "uinput access";
const TRAY_HOST: &str = "AppIndicator extension";

/// What `missing_permissions` can report, for `--check`.
pub const PERMISSIONS: &[&str] = &[INPUT_GROUP, UINPUT, TRAY_HOST];

/// Nothing to open on Linux: scripts/install-linux.sh fixes all three.
pub fn permission_help(_name: &str) -> Option<&'static str> {
    None
}

const INPUT_HELP: &str = "cannot read /dev/input: add yourself to the input group \
     (scripts/install-linux.sh does), then log out and back in";

/// The compositor gives focus back to the previous window once the panel
/// hides, in about this long (measured on GNOME).
const REFOCUS: Duration = Duration::from_millis(150);
/// PRIMARY changed this recently: a selection just made, read as it is.
const FRESH: Duration = Duration::from_secs(15);
/// How long the app in front gets to copy on the copy keys. Not measured.
const COPY_WAIT: Duration = Duration::from_millis(500);
/// Off-layout text: let the compositor see the new clipboard owner before
/// the paste, and let the app fetch the text before the next key or chunk.
/// Not measured.
const CLIP_SETTLE: Duration = Duration::from_millis(50);
const PASTE_FETCH: Duration = Duration::from_millis(150);

pub struct Linux {
    /// The compositor's clipboard without focus; None: it offers none (GNOME).
    control: Option<control::Control>,
    held: keys::Held,
    keyboard: Mutex<Option<Keyboard>>,
    scheme: scheme::Scheme,
    x11: clip::X11,
}

/// The `[linux]` table. `platform::current()` takes no config, so it is read
/// here; a broken file means the defaults, as in the daemon.
fn settings() -> LinuxConfig {
    Paths::user()
        .and_then(|p| Config::load(&p))
        .map(|c| c.linux)
        .unwrap_or_default()
}

impl Linux {
    /// Creates the uinput keyboard now: the compositor needs ~200 ms to
    /// adopt it, which must not delay the first dictation.
    pub fn new() -> Linux {
        let cfg = settings();
        let keyboard = typing::keys_of(&cfg)
            .and_then(Keyboard::new)
            .inspect(|k| log::info!("typing: {}", k.keys.layout))
            .inspect_err(|e| log::error!("no typing: {e}"))
            .ok();
        Linux {
            control: control::Control::new(),
            held: keys::Held::default(),
            keyboard: Mutex::new(keyboard),
            scheme: scheme::Scheme::follow(),
            x11: clip::X11::default(),
        }
    }

    fn with_keyboard<T>(&self, f: impl FnOnce(&mut Keyboard) -> Result<T>) -> Result<T> {
        match lock(&self.keyboard).as_mut() {
            Some(kb) => f(kb),
            None => Err(Error::Platform(
                "cannot type: no uinput keyboard (see blitznir.log)".into(),
            )),
        }
    }

    /// `send` (paste_keys, undo_keys) once the focus is back where it was;
    /// false: no keyboard.
    fn shortcut(&self, send: fn(&mut Keyboard) -> std::io::Result<()>) -> Result<bool> {
        if lock(&self.keyboard).is_none() {
            return Ok(false);
        }
        thread::sleep(REFOCUS);
        self.wait_modifiers_released();
        self.with_keyboard(|kb| send(kb).map_err(io).map(|()| true))
    }

    /// Characters the layout cannot type: the clipboard (the compositor's
    /// data control, else X11) + paste_keys.
    fn paste_chunk(&self, kb: &mut Keyboard, text: &str) -> Result<()> {
        let set = match &self.control {
            Some(c) => c.set(text),
            None => self.x11.set(text),
        };
        if let Err(e) = set {
            log::warn!(
                "dropped {text:?}: not on the {:?} layout, and no clipboard to paste it from \
                 ({e}); set [linux] xkb_layout to your keyboard's layout",
                kb.keys.layout
            );
            return Ok(());
        }
        thread::sleep(CLIP_SETTLE);
        kb.paste()?;
        thread::sleep(PASTE_FETCH);
        Ok(())
    }
}

fn io(e: std::io::Error) -> Error {
    Error::Platform(format!("uinput: {e}"))
}

impl Platform for Linux {
    fn listen(&self, tx: Sender<KeyEvent>) -> Result<()> {
        if keys::input_denied() {
            return Err(Error::Platform(INPUT_HELP.into()));
        }
        keys::spawn(tx, self.held.clone()).map_err(Error::Io)
    }

    fn type_text(&self, s: &str) -> Result<()> {
        self.with_keyboard(|kb| {
            for run in typing::runs(kb.map(), s) {
                match run {
                    Run::Keys(t) => kb.type_keys(t).map_err(io)?,
                    Run::Paste(t) => self.paste_chunk(kb, t)?,
                }
            }
            Ok(())
        })
    }

    fn backspace(&self, n: usize) -> Result<()> {
        self.with_keyboard(|kb| kb.backspace(n).map_err(io))
    }

    fn wait_modifiers_released(&self) {
        let end = Instant::now() + Duration::from_secs(2);
        while self.held.any() && Instant::now() < end {
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn focused_app(&self) -> Focus {
        Focus(None)
    }

    fn focused_editable(&self, _f: &Focus) -> Option<bool> {
        None
    }

    /// A fresh PRIMARY is the selection, and no keys are sent. Else, where
    /// the compositor tells of every copy and selection (data control v2),
    /// `[linux] copy_keys` ask the app in front to copy: an app may share no
    /// PRIMARY (COSMIC's own), and an old one is likely someone else's.
    /// Only when no selection was made in the last [`FRESH`]: in a terminal
    /// that does not copy on them they come out as text ("5~"); an editor
    /// that copies the line where nothing is selected hands over that line.
    fn begin_copy(&self) -> CopyToken {
        let Some(c) = self.control.as_ref().filter(|c| c.tells_primary()) else {
            return CopyToken::NONE;
        };
        if c.primary_fresh(FRESH) {
            log::info!("selection: PRIMARY, changed in the last {FRESH:?}");
            return CopyToken::NONE;
        }
        self.wait_modifiers_released(); // a held Shift would change the keys
        let before = c.changes();
        match self.with_keyboard(|kb| kb.copy().map_err(io)) {
            Ok(true) => {
                log::info!("selection: PRIMARY is older than {FRESH:?}: the copy keys");
                CopyToken(i64::try_from(before).unwrap_or(0))
            }
            Ok(false) => CopyToken::NONE,
            Err(e) => {
                log::warn!("selection: {e}");
                CopyToken::NONE
            }
        }
    }

    /// The copy's text once the clipboard changes; None if it does not (the
    /// app had nothing selected, or does not copy on those keys): PRIMARY
    /// is old then, not the selection. No copy sent: PRIMARY, which goes to
    /// the focused client: waits up to 1 s for the panel's focus.
    fn finish_copy(&self, t: CopyToken) -> Option<String> {
        let Some(c) = self.control.as_ref().filter(|_| t.sent()) else {
            return clip::primary();
        };
        let changed = || i64::try_from(c.changes()).unwrap_or(0) != t.0;
        let end = Instant::now() + COPY_WAIT;
        while !changed() && Instant::now() < end {
            thread::sleep(Duration::from_millis(20));
        }
        if !changed() {
            log::info!("selection: the copy keys copied nothing in {COPY_WAIT:?}");
            return None;
        }
        c.clipboard_text()
    }

    /// Wayland tells no client where another one's windows are, let alone
    /// its selection; lightning aims at the mouse.
    fn selection_bounds(&self, _f: &Focus) -> Option<Bounds> {
        None
    }

    fn selected_text(&self, _f: &Focus) -> Option<String> {
        None
    }

    fn text_before_caret(&self, _f: &Focus) -> Option<String> {
        None
    }

    fn caret(&self, _f: &Focus) -> Option<Rect> {
        None
    }

    fn mouse(&self) -> Option<(f64, f64)> {
        None
    }

    /// The compositor's data control where it offers one: no focus needed.
    /// Else, with the panel focused, GTK alone: the native path, which
    /// Mutter takes from the focused client. Otherwise X11 first (XWayland
    /// serves it without focus), then GTK too (an X11 session takes it from
    /// anyone).
    fn set_clipboard(&self, s: &str) -> Result<()> {
        if let Some(c) = &self.control {
            match c.set(s) {
                Ok(()) => return Ok(()),
                Err(e) => log::warn!("{e}: the X11 clipboard instead"),
            }
        }
        let x11 = match clip::focused() {
            Some(true) => Ok(()),
            _ => self.x11.set(s),
        };
        if clip::set_gtk(s) {
            return Ok(());
        }
        x11.map_err(|e| Error::Platform(format!("no clipboard: no GTK main loop, X11: {e}")))
    }

    /// GTK's while we have focus, else X11's.
    fn clipboard_image(&self) -> Option<Vec<u8>> {
        clip::image().or_else(|| self.x11.image())
    }

    /// As [`Platform::set_clipboard`] does the text, bar the data-control path.
    fn set_clipboard_image(&self, png: &[u8]) -> Result<()> {
        let x11 = match clip::focused() {
            Some(true) => Ok(()),
            _ => self.x11.set_image(png),
        };
        if clip::set_gtk_image(png) {
            return Ok(());
        }
        x11.map_err(|e| Error::Platform(format!("no clipboard: no GTK main loop, X11: {e}")))
    }

    /// Blocks ~150 ms: call it off the main thread, after the panel hid.
    fn paste_into(&self, _f: &Focus) -> Result<bool> {
        self.shortcut(Keyboard::paste)
    }

    /// No window can be brought back on Wayland; the compositor gives the
    /// focus back when the overlay goes.
    fn refocus(&self, _f: &Focus) -> bool {
        true
    }

    /// The revert offer is macOS only: Wayland places no window.
    fn show_quietly(&self) {}

    /// The Settings portal's color-scheme, as last heard: Wayland gives a
    /// window no theme.
    fn prefers_dark(&self) -> Option<bool> {
        self.scheme.dark()
    }

    /// The new `[linux]` keys on the same uinput keyboard (no new device for
    /// the compositor to adopt), or a first one where the start had none (a
    /// layout that did not exist then). A bad one keeps the last. Built
    /// before the keyboard is locked: typing holds it, and the desktop's
    /// keymap takes a roundtrip.
    fn configure(&self, cfg: &Config) {
        let keys = match typing::keys_of(&cfg.linux) {
            Ok(keys) => keys,
            Err(e) => return log::error!("typing: {e}; the last layout stays"),
        };
        log::info!("typing: {}", keys.layout);
        let mut kb = lock(&self.keyboard);
        match kb.as_mut() {
            Some(k) => k.keys = keys,
            None => match Keyboard::new(keys) {
                Ok(k) => *kb = Some(k),
                Err(e) => log::error!("no typing: {e}"),
            },
        }
    }

    /// No window can be brought back on Wayland: in whatever has the focus.
    fn backspace_into(&self, _f: &Focus, n: usize) -> Result<bool> {
        self.backspace(n)?;
        Ok(true)
    }

    /// No window can be brought back on Wayland: to whatever has the focus.
    fn undo(&self, _f: &Focus) -> Result<bool> {
        self.shortcut(Keyboard::undo)
    }

    /// As `undo`: Return where the focus is.
    fn send(&self, _f: &Focus) -> Result<bool> {
        self.shortcut(Keyboard::enter)
    }

    /// `[capture] cmd = ""` (the Linux default): the desktop's screenshot portal.
    fn capture(&self, cmd: &str, out: &Path) -> Result<bool> {
        if cmd.trim().is_empty() {
            return portal::screenshot(out).map_err(Error::Platform);
        }
        super::run_capture(cmd, out)
    }

    /// A link in an answer or the release page; never a grant (none to open).
    fn open_url(&self, url: &str) -> Result<()> {
        super::spawn_reaped("xdg-open", url)
    }

    fn play_sound(&self, path: &Path) -> Result<Option<std::process::Child>> {
        Ok(Some(
            std::process::Command::new("pw-play").arg(path).spawn()?,
        ))
    }

    fn missing_permissions(&self) -> Vec<&'static str> {
        let missing = [
            keys::input_denied(),
            lock(&self.keyboard).is_none(),
            !clip::tray_host(),
        ];
        PERMISSIONS
            .iter()
            .zip(missing)
            .filter(|(_, m)| *m)
            .map(|(name, _)| *name)
            .collect()
    }
}
