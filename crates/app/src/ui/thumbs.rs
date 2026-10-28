//! Images as thumbnails: attached to the composer (with a Remove) and in
//! a chat's messages.

use super::icons::{self, icon_button};
use super::style::Palette;
use crate::panel::Panel;
use crate::panel::view::Shot;
use freya::prelude::*;
use std::sync::Arc;

/// A thumbnail's height; its width follows the image's proportions within
/// [`THUMB_W`], so the Remove on its corner sits on the picture itself, not
/// beside an empty frame (a tall screenshot filled a sliver of 160 pt).
const THUMB_H: f32 = 90.0;
const THUMB_W: (f32, f32) = (40.0, 240.0);

/// The width for `png` at [`THUMB_H`], from its header alone.
fn thumb_width(png: &[u8]) -> f32 {
    let reader = image::ImageReader::new(std::io::Cursor::new(png)).with_guessed_format();
    let dims = reader.ok().and_then(|r| r.into_dimensions().ok());
    dims.map_or(160.0, |(w, h)| {
        (THUMB_H * w as f32 / h.max(1) as f32).clamp(THUMB_W.0, THUMB_W.1)
    })
}

/// Attached and stored images, as thumbnails; attached ones have a Remove
/// on their top-right corner.
pub fn thumbs(p: &Palette, shots: &[Shot], remove: Option<&Arc<Panel>>) -> Option<Element> {
    if shots.is_empty() {
        return None;
    }
    let thumb = |s: &Shot| {
        // covering its box, centred: past the width limits it is cropped,
        // never letterboxed away from the Remove on its corner
        let image = ImageViewer::new((s.id, s.png.clone()))
            .width(Size::fill())
            .height(Size::fill())
            .aspect_ratio(AspectRatio::Max)
            .image_cover(ImageCover::Center)
            .corner_radius(4.0);
        let x = remove.map(|panel| {
            let (panel, id) = (panel.clone(), s.id);
            let button = icon_button(p, icons::X, "Remove", true, move |_| panel.remove_image(id));
            // a dark disc: the × reads on any picture
            rect()
                .position(Position::new_absolute().top(3.0).right(3.0))
                .corner_radius(99.0)
                .background(Color::from_argb(170, 0, 0, 0))
                .color(Color::WHITE)
                .child(button)
        });
        rect()
            .key(s.id)
            .width(Size::px(thumb_width(&s.png)))
            .height(Size::px(THUMB_H))
            .child(image)
            .maybe_child(x)
            .into_element()
    };
    let row = rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::wrap())
        .spacing(6.0)
        .children(shots.iter().map(thumb));
    Some(row.into_element())
}
