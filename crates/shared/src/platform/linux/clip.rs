//! PRIMARY and CLIPBOARD. GTK calls run on the GTK main thread, which is
//! Freya's tray thread; arboard is the X11 path (XWayland on GNOME).
//!
//! Mutter has no data-control protocol and hands the Wayland selection only
//! to the focused client: GTK works while the panel has focus, and from
//! the background only X11 through XWayland is left.

use blitznir_core::lock;
use gtk::glib::{self, ToVariant};
use gtk::prelude::{Cast, GtkWindowExt};
use gtk::{gdk, gdk_pixbuf, gio};
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// How long a caller waits for the main thread; a nested clipboard read on a
/// busy loop may take this long, a stuck one is given up.
const MAIN_WAIT: Duration = Duration::from_secs(2);

/// Run `f` on the GTK main thread and wait for its answer. None: no GTK
/// (the CLI), or the main thread did not answer in time.
fn on_main<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    if gtk::is_initialized_main_thread() {
        return Some(f());
    }
    if !gtk::is_initialized() {
        return None;
    }
    let (tx, rx) = mpsc::sync_channel(1);
    // an idle source, never g_main_context_invoke: invoke runs `f` on the
    // CALLING thread whenever it can acquire the context
    glib::idle_add_once(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(MAIN_WAIT).ok()
}

/// How long [`primary`] waits for the panel to get focus: it shows
/// asynchronously and re-asserts focus 250 ms later (tray::show_panel).
const FOCUS_WAIT: Duration = Duration::from_secs(1);

/// One of our windows has keyboard focus. None: no GTK.
pub fn focused() -> Option<bool> {
    on_main(|| {
        gtk::Window::list_toplevels()
            .iter()
            .filter_map(|w| w.downcast_ref::<gtk::Window>())
            .any(GtkWindowExt::is_active)
    })
}

/// The selection. Wayland sends PRIMARY to the focused client only, so this
/// waits for our focus first (except on the main thread, which it would block).
pub fn primary() -> Option<String> {
    let end = Instant::now() + FOCUS_WAIT;
    while !gtk::is_initialized_main_thread() && focused() == Some(false) && Instant::now() < end {
        thread::sleep(Duration::from_millis(25));
    }
    on_main(|| {
        gtk::Clipboard::get(&gdk::SELECTION_PRIMARY)
            .wait_for_text()
            .map(|s| s.to_string())
    })
    .flatten()
    .filter(|s| !s.is_empty())
}

/// GTK's CLIPBOARD: native on Wayland, but taken only while we have focus.
/// false: no GTK main loop to run it on.
pub fn set_gtk(text: &str) -> bool {
    let text = text.to_string();
    on_main(move || gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_text(&text)).is_some()
}

/// GTK's CLIPBOARD image as PNG; like the text, only while we have focus.
pub fn image() -> Option<Vec<u8>> {
    on_main(|| {
        let pixbuf = gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).wait_for_image()?;
        pixbuf.save_to_bufferv("png", &[]).ok()
    })
    .flatten()
}

/// `png` on GTK's CLIPBOARD; like the text, taken only while we have focus.
/// false: no GTK main loop, or not an image.
pub fn set_gtk_image(png: &[u8]) -> bool {
    let bytes = glib::Bytes::from(png);
    on_main(move || {
        let stream = gio::MemoryInputStream::from_bytes(&bytes);
        let pixbuf = gdk_pixbuf::Pixbuf::from_stream(&stream, gio::Cancellable::NONE).ok()?;
        gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_image(&pixbuf);
        Some(())
    })
    .flatten()
    .is_some()
}

/// The X11 CLIPBOARD owner. It must stay alive: X11 serves a selection from
/// the owner's process, so the text is gone when it is dropped.
#[derive(Default)]
pub struct X11(Mutex<Option<arboard::Clipboard>>);

impl X11 {
    /// Needs an X server (XWayland on a Wayland session).
    pub fn set(&self, text: &str) -> Result<(), String> {
        let mut cb = lock(&self.0);
        if cb.is_none() {
            *cb = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
        }
        let set = cb
            .as_mut()
            .map(|c| c.set_text(text).map_err(|e| e.to_string()));
        set.unwrap_or_else(|| Err("no X11 clipboard".into()))
    }

    /// `png` on the X11 CLIPBOARD, as arboard's RGBA.
    pub fn set_image(&self, png: &[u8]) -> Result<(), String> {
        let rgba = image::load_from_memory(png)
            .map_err(|e| e.to_string())?
            .into_rgba8();
        let (width, height) = (rgba.width() as usize, rgba.height() as usize);
        let img = arboard::ImageData {
            width,
            height,
            bytes: rgba.into_raw().into(),
        };
        let mut cb = lock(&self.0);
        if cb.is_none() {
            *cb = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
        }
        let set = cb
            .as_mut()
            .map(|c| c.set_image(img).map_err(|e| e.to_string()));
        set.unwrap_or_else(|| Err("no X11 clipboard".into()))
    }

    /// The image on the X11 CLIPBOARD as PNG: XWayland serves one a Wayland
    /// app copied, with no focus of ours (GTK's needs it, and under Freya
    /// never has it).
    pub fn image(&self) -> Option<Vec<u8>> {
        let mut cb = lock(&self.0);
        if cb.is_none() {
            *cb = arboard::Clipboard::new().ok();
        }
        let img = cb.as_mut()?.get_image().ok()?;
        let (w, h) = (
            u32::try_from(img.width).ok()?,
            u32::try_from(img.height).ok()?,
        );
        let rgba = image::RgbaImage::from_raw(w, h, img.bytes.into_owned())?;
        let mut png = std::io::Cursor::new(Vec::new());
        rgba.write_to(&mut png, image::ImageFormat::Png).ok()?;
        Some(png.into_inner())
    }
}

/// A StatusNotifierItem host is on the session bus: KDE has one, GNOME only
/// with the AppIndicator extension. Without it our tray icon shows nowhere.
pub fn tray_host() -> bool {
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return false;
    };
    let reply = bus.call_sync(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        Some(&("org.kde.StatusNotifierWatcher",).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        500,
        gio::Cancellable::NONE,
    );
    reply
        .ok()
        .and_then(|v| v.get::<(bool,)>())
        .is_some_and(|(has,)| has)
}

#[cfg(test)]
mod tests {
    #[test]
    fn without_gtk_there_is_no_primary_and_no_gtk_clipboard() {
        // tests never initialize GTK, like the CLI
        assert_eq!(super::focused(), None);
        assert_eq!(super::primary(), None);
        assert_eq!(super::image(), None);
        assert!(!super::set_gtk("x"));
    }

    #[test]
    fn the_container_has_no_display_and_no_session_bus() {
        if std::path::Path::new("/.dockerenv").exists() {
            assert!(super::X11::default().set("x").is_err());
            assert!(!super::tray_host());
        }
    }
}
