//! The images for the next message: screen capture and image paste; the
//! Copy buttons (text and images) and an answer's links.

use super::view::Shot;
use super::{BUSY, Panel, spawn};
use blitznir_core::{lock, store};
use freya::prelude::Bytes;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The panel's hide takes effect asynchronously; keep it out of the shot.
const HIDE_SETTLE: Duration = Duration::from_millis(250);
/// Pasted and captured images go to the model at this size at most.
const IMAGE_MAX_PX: u32 = 1280;

/// `png` no larger than [`IMAGE_MAX_PX`] on either side, as PNG; None: it
/// is no larger already.
fn downscale(png: &[u8]) -> std::result::Result<Option<Vec<u8>>, String> {
    let img = image::load_from_memory(png).map_err(|e| e.to_string())?;
    if img.width().max(img.height()) <= IMAGE_MAX_PX {
        return Ok(None);
    }
    let filter = image::imageops::FilterType::Triangle;
    let img = img.resize(IMAGE_MAX_PX, IMAGE_MAX_PX, filter);
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(Some(out.into_inner()))
}

impl Panel {
    /// A new image to show; `full`: as attached, when larger than `png`.
    pub(super) fn shot(&self, url: String, png: Vec<u8>, full: Option<Vec<u8>>) -> Shot {
        let id = self.shots.fetch_add(1, Ordering::Relaxed);
        let (png, full) = (Bytes::from(png), full.map(Bytes::from));
        let stored = None;
        Shot {
            id,
            url,
            png,
            full,
            stored,
        }
    }

    /// `shot` at its largest: as attached, read from images/ for a stored one.
    pub fn largest(&self, shot: &Shot) -> Bytes {
        if let Some(full) = &shot.full {
            return full.clone();
        }
        let original = shot.stored.as_deref().and_then(|n| self.store.original(n));
        original.map_or_else(|| shot.png.clone(), Bytes::from)
    }

    /// Store `shot` as image `name`: what the model gets, and the original.
    pub(super) fn put_shot(&self, name: &str, shot: &Shot) -> blitznir_core::Result<()> {
        self.store.put_image(name, &shot.url)?;
        match &shot.full {
            Some(full) => self.store.put_original(name, full),
            None => Ok(()),
        }
    }

    /// A Copy button, on a thread of its own: on Linux the clipboard waits
    /// for the GTK thread.
    pub fn copy(self: &Arc<Self>, text: String, said: String) {
        let me = self.clone();
        let copy = move || match me.platform.set_clipboard(&text) {
            Ok(()) => me.say(said),
            Err(e) => me.fail(format!("Copy failed: {e}")),
        };
        if let Err(e) = spawn("copy", copy) {
            self.fail(format!("Copy failed: {e}"));
        }
    }

    /// A link's open button in an answer.
    pub fn open_link(&self, url: &str) {
        if let Err(e) = self.platform.open_url(url) {
            self.fail(format!("Could not open {url}: {e}"));
        }
    }

    /// A chat image's Copy: the image as attached, on the clipboard.
    pub fn copy_shot(self: &Arc<Self>, shot: Shot) {
        let me = self.clone();
        let copy = move || match me.platform.set_clipboard_image(&me.largest(&shot)) {
            Ok(()) => me.say("Image copied"),
            Err(e) => me.fail(format!("Copy failed: {e}")),
        };
        if let Err(e) = spawn("copy-image", copy) {
            self.fail(format!("Copy failed: {e}"));
        }
    }

    /// Attach `png` (downscaled, the original kept) for the next Return.
    fn add_image(&self, png: &[u8]) {
        let (png, full) = match downscale(png) {
            Ok(Some(small)) => (small, Some(png.to_vec())),
            Ok(None) => (png.to_vec(), None),
            Err(e) => return self.fail(format!("Not attached: {e}")),
        };
        let shot = self.shot(store::data_url("image/png", &png), png, full);
        self.with_ui(|ui| {
            ui.images.push(shot);
            ui.status.clear();
        });
    }

    pub fn remove_image(&self, id: u64) {
        self.with_ui(|ui| ui.images.retain(|s| s.id != id));
    }

    /// The image button, ⌘V with an image: whatever image the clipboard holds.
    pub fn paste_image(self: &Arc<Self>, quiet: bool) {
        // off this thread: Wayland's read waits on the event loop
        let me = self.clone();
        let spawned = spawn("paste-image", move || match me.platform.clipboard_image() {
            Some(png) => me.add_image(&png),
            None if !quiet => me.fail("No image on the clipboard: copy one first"),
            None => {}
        });
        if let Err(e) = spawned {
            self.fail(e);
        }
    }

    /// Capture: hide, let the OS selector run, come back with the shot.
    pub fn capture(self: &Arc<Self>) {
        if lock(&self.view).job.busy() {
            return self.say(BUSY);
        }
        self.say("Select a region… (Esc cancels)");
        let cmd = self.cfg().capture.cmd;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path: PathBuf =
            std::env::temp_dir().join(format!("blitznir-{}-{stamp}.png", std::process::id()));
        let me = self.clone();
        let spawned = spawn("capture", move || {
            me.hide_and_wait();
            thread::sleep(HIDE_SETTLE);
            let shot = me.platform.capture(&cmd, &path);
            me.show();
            match shot.map(|taken| taken.then(|| std::fs::read(&path))) {
                Ok(Some(Ok(png))) => {
                    log::info!("capture: {} bytes", png.len());
                    me.add_image(&png);
                }
                Ok(None) => me.say("Capture cancelled"),
                Ok(Some(Err(e))) => me.fail(format!("Capture failed: {e}")),
                Err(e) => me.fail(format!("Capture failed: {e}")),
            }
            let _ = std::fs::remove_file(&path);
        });
        if let Err(e) = spawned {
            self.fail(e);
        }
    }
}
