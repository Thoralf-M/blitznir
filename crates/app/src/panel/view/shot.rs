//! An image of a chat or the composer.

use freya::prelude::Bytes;

/// An image: the data URL goes to the model (empty for a stored one), the
/// PNG to the screen, downscaled as sent; `full` is the image as attached
/// when that was larger, in memory until it is stored, then read from
/// images/ (`stored`, its name there) only when the large view or Copy
/// wants it ([`crate::panel::Panel::largest`]). Equal by id: comparing the
/// pixels on every render would cost more than drawing them.
#[derive(Clone)]
pub struct Shot {
    pub id: u64,
    pub url: String,
    pub png: Bytes,
    pub full: Option<Bytes>,
    pub stored: Option<String>,
}

impl PartialEq for Shot {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
