//! The images for the next message: screen capture and image paste.

use super::{BUSY, Panel, spawn};
use blitznir_core::{lock, store};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The panel's hide takes effect asynchronously; keep it out of the shot.
const HIDE_SETTLE: Duration = Duration::from_millis(250);
/// Pasted and captured images go to the model at this size at most.
const IMAGE_MAX_PX: u32 = 1280;

/// `png` no larger than [`IMAGE_MAX_PX`] on either side, as PNG.
fn downscale(png: &[u8]) -> std::result::Result<Vec<u8>, String> {
    let img = image::load_from_memory(png).map_err(|e| e.to_string())?;
    let img = if img.width().max(img.height()) > IMAGE_MAX_PX {
        img.resize(
            IMAGE_MAX_PX,
            IMAGE_MAX_PX,
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

impl Panel {
    /// Attach `png` (downscaled) for the next Return.
    fn add_image(&self, png: &[u8]) {
        let png = match downscale(png) {
            Ok(png) => png,
            Err(e) => return self.fail(format!("Not attached: {e}")),
        };
        let shot = self.shot(store::data_url("image/png", &png), png);
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
