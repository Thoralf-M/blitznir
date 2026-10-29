//! Linux: the shared platform with the selection on winit's Wayland
//! connection. Mutter offers PRIMARY to, and takes CLIPBOARD from, only the
//! client whose surface has keyboard focus. Under Freya that is winit's
//! connection, never GTK's, so the shared GTK path finds nothing;
//! copypasta (smithay-clipboard, Freya's own) on winit's `wl_display`
//! does. Everything else, and anything while no window of ours has focus
//! (the X11 path), is the shared platform's. The Freya shell's only OS code.
#![allow(unsafe_code)]

use blitznir_core::platform::{Bounds, CopyToken, Focus, KeyEvent, Platform, Rect};
use blitznir_core::{Result, lock};
use copypasta::ClipboardProvider;
use copypasta::wayland_clipboard::{Clipboard, Primary, create_clipboards_from_external};
use freya::prelude::{LaunchProxy, RendererContext};
use freya::winit::event_loop::OwnedDisplayHandle;
use freya::winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use std::path::Path;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

/// How long [`Platform::finish_copy`] waits for the panel's focus: it is
/// made and shown asynchronously and asked for focus again 250 ms later.
const FOCUS_WAIT: Duration = Duration::from_secs(1);
/// The PRIMARY offer follows the focus by a few ms (2–74 ms measured).
const OFFER_WAIT: Duration = Duration::from_millis(300);
const POLL: Duration = Duration::from_millis(25);
/// smithay-clipboard sets the selection on its own thread; a window
/// destroyed before it did would lose the focus it needs.
const CLIP_SETTLE: Duration = Duration::from_millis(50);
/// A question to the event loop gives up after this.
const LOOP_WAIT: Duration = Duration::from_millis(500);

/// PRIMARY and CLIPBOARD on winit's connection. Fields drop in order: the
/// clipboards stop their thread before the display handle lets go.
struct Selection {
    primary: Primary,
    clipboard: Clipboard,
    _display: OwnedDisplayHandle,
}

pub struct Wayland {
    inner: Arc<dyn Platform>,
    proxy: LaunchProxy,
    /// None on X11, where the shared platform does it all.
    sel: Mutex<Option<Selection>>,
}

impl Wayland {
    /// Wraps `inner` and makes the selection pair on the event loop as soon
    /// as it runs, before any window of ours can take the focus the first
    /// PRIMARY offer comes with.
    pub fn new(inner: Arc<dyn Platform>, proxy: LaunchProxy) -> Arc<Wayland> {
        let me = Arc::new(Wayland {
            inner,
            proxy: proxy.clone(),
            sel: Mutex::new(None),
        });
        let weak = Arc::downgrade(&me);
        drop(proxy.post_callback(move |ctx| init(&weak, ctx)));
        me
    }

    fn wayland(&self) -> bool {
        lock(&self.sel).is_some()
    }

    /// One of our windows has keyboard focus. Never on the event loop's
    /// thread: it waits for it.
    fn focused(&self) -> bool {
        let (tx, rx) = mpsc::sync_channel(1);
        drop(self.proxy.post_callback(move |ctx| {
            let _ = tx.send(ctx.windows().values().any(|w| w.window().has_focus()));
        }));
        rx.recv_timeout(LOOP_WAIT).unwrap_or(false)
    }

    fn primary(&self) -> Option<String> {
        let mut sel = lock(&self.sel);
        let text = sel.as_mut()?.primary.get_contents().ok()?;
        (!text.is_empty()).then_some(text)
    }
}

fn init(me: &Weak<Wayland>, ctx: &mut RendererContext) {
    let Some(me) = me.upgrade() else {
        return;
    };
    let display = ctx.active_event_loop.owned_display_handle();
    let Ok(handle) = display.display_handle() else {
        return;
    };
    let RawDisplayHandle::Wayland(wl) = handle.as_raw() else {
        log::info!("not on Wayland: the selection goes through GTK and X11");
        return;
    };
    // SAFETY: a live wl_display, kept alive by `display` for as long as
    // the clipboards made on it (freya-winit's clipboard.rs does the same).
    let (primary, clipboard) = unsafe { create_clipboards_from_external(wl.display.as_ptr()) };
    *lock(&me.sel) = Some(Selection {
        primary,
        clipboard,
        _display: display,
    });
    log::info!("selection on winit's Wayland connection");
}

impl Platform for Wayland {
    /// PRIMARY once a window of ours has focus: the panel, shown for this.
    fn finish_copy(&self, t: CopyToken) -> Option<String> {
        // X11; or the copy keys went out because PRIMARY is old: their copy or nothing
        if !self.wayland() || t.sent() {
            return self.inner.finish_copy(t);
        }
        let end = Instant::now() + FOCUS_WAIT;
        while !self.focused() && Instant::now() < end {
            thread::sleep(POLL);
        }
        let offered = Instant::now() + OFFER_WAIT;
        loop {
            if let Some(text) = self.primary() {
                return Some(text);
            }
            if Instant::now() >= offered {
                return None;
            }
            thread::sleep(POLL);
        }
    }

    /// Ours while a window of ours has focus; the shared X11 path otherwise.
    fn set_clipboard(&self, s: &str) -> Result<()> {
        if !(self.wayland() && self.focused()) {
            return self.inner.set_clipboard(s);
        }
        if let Some(sel) = lock(&self.sel).as_mut() {
            // copypasta's Wayland store only queues the text: it cannot fail
            let _ = sel.clipboard.set_contents(s.to_string());
        }
        thread::sleep(CLIP_SETTLE);
        Ok(())
    }

    /// smithay-clipboard reads text only: images take the shared path.
    fn clipboard_image(&self) -> Option<Vec<u8>> {
        self.inner.clipboard_image()
    }

    fn listen(&self, tx: Sender<KeyEvent>) -> Result<()> {
        self.inner.listen(tx)
    }

    fn type_text(&self, s: &str) -> Result<()> {
        self.inner.type_text(s)
    }

    fn backspace(&self, n: usize) -> Result<()> {
        self.inner.backspace(n)
    }

    fn wait_modifiers_released(&self) {
        self.inner.wait_modifiers_released();
    }

    fn focused_app(&self) -> Focus {
        self.inner.focused_app()
    }

    fn focused_editable(&self, f: &Focus) -> Option<bool> {
        self.inner.focused_editable(f)
    }

    fn begin_copy(&self) -> CopyToken {
        self.inner.begin_copy()
    }

    fn selection_bounds(&self, f: &Focus) -> Option<Bounds> {
        self.inner.selection_bounds(f)
    }

    fn caret(&self, f: &Focus) -> Option<Rect> {
        self.inner.caret(f)
    }

    fn mouse(&self) -> Option<(f64, f64)> {
        self.inner.mouse()
    }

    fn paste_into(&self, f: &Focus) -> Result<bool> {
        self.inner.paste_into(f)
    }

    fn refocus(&self, f: &Focus) -> bool {
        self.inner.refocus(f)
    }

    fn undo(&self, f: &Focus) -> Result<bool> {
        self.inner.undo(f)
    }

    fn show_quietly(&self) {
        self.inner.show_quietly();
    }

    fn configure(&self, cfg: &blitznir_core::config::Config) {
        self.inner.configure(cfg);
    }

    fn prefers_dark(&self) -> Option<bool> {
        self.inner.prefers_dark()
    }

    fn backspace_into(&self, f: &Focus, n: usize) -> Result<bool> {
        self.inner.backspace_into(f, n)
    }

    fn capture(&self, cmd: &str, out: &Path) -> Result<bool> {
        self.inner.capture(cmd, out)
    }

    fn missing_permissions(&self) -> Vec<&'static str> {
        self.inner.missing_permissions()
    }

    fn open_url(&self, url: &str) -> Result<()> {
        self.inner.open_url(url)
    }

    fn play_sound(&self, path: &Path) -> Result<Option<std::process::Child>> {
        self.inner.play_sound(path)
    }
}
