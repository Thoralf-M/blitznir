//! A chat's images in images/: `<id>.url`, the data URL sent to the model
//! (downscaled), and beside it `<id>.png`, the image as attached, when
//! that was larger: what the large view shows and Copy copies.

use super::{Message, Store, write_atomic};
use crate::Result;
use std::path::{Path, PathBuf};

impl Store {
    /// A fresh name for an attached image: named when sent, written later.
    pub fn image_name() -> String {
        format!("{}.url", uuid::Uuid::new_v4().simple())
    }

    /// Store an attached image (the data URL the panel sends) as `name`.
    pub fn put_image(&self, name: &str, data_url: &str) -> Result<()> {
        std::fs::create_dir_all(self.paths.images())?;
        let path = self.paths.images().join(name);
        Ok(write_atomic(&path, data_url.as_bytes())?)
    }

    pub fn image(&self, name: &str) -> Result<String> {
        let name = Path::new(name).file_name().unwrap_or_default(); // never outside images/
        Ok(std::fs::read_to_string(self.paths.images().join(name))?)
    }

    /// Image `name` as attached, before it was downscaled for the model.
    pub fn put_original(&self, name: &str, png: &[u8]) -> Result<()> {
        std::fs::create_dir_all(self.paths.images())?;
        let path = self.original_path(name);
        let path = path.ok_or_else(|| std::io::Error::other(format!("bad image name {name}")))?;
        Ok(write_atomic(&path, png)?)
    }

    /// Image `name` as attached; None: it was sent as it came.
    pub fn original(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.original_path(name)?).ok()
    }

    /// `<id>.png` beside `<id>.url`, never outside images/.
    fn original_path(&self, name: &str) -> Option<PathBuf> {
        let file = Path::new(Path::new(name).file_name()?).with_extension("png");
        Some(self.paths.images().join(file))
    }

    /// The data URLs of a message's images; a missing file is skipped.
    pub fn message_images(&self, m: &Message) -> Vec<String> {
        m.images
            .iter()
            .filter_map(|n| self.image(n).map_err(|e| log::warn!("image {n}: {e}")).ok())
            .collect()
    }

    /// Delete the image files of `messages`, originals included.
    pub fn remove_images<'a>(&self, messages: impl IntoIterator<Item = &'a Message>) {
        for name in messages.into_iter().flat_map(|m| &m.images) {
            if let Some(file) = Path::new(name).file_name() {
                let _ = std::fs::remove_file(self.paths.images().join(file));
            }
            if let Some(original) = self.original_path(name) {
                let _ = std::fs::remove_file(original);
            }
        }
    }
}
