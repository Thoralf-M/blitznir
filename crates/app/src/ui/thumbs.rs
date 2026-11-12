//! Images as thumbnails: attached to the composer (with a Remove) and in
//! a chat's messages (with a Copy). A press on one shows it large over the
//! window (viewer.rs).

use super::{
    icons::{self, icon_button},
    style::Palette,
};
use crate::panel::{Panel, view::Shot};
use freya::prelude::*;
use std::sync::Arc;

/// A thumbnail's height; its width follows the image's proportions within
/// [`THUMB_W`], so the Remove on its corner sits on the picture itself, not
/// beside an empty frame (a tall screenshot filled a sliver of 160 pt).
const THUMB_H: f32 = 90.0;
const THUMB_W: (f32, f32) = (40.0, 240.0);

/// The image shown large, if any: the window's (`app` provides it).
#[derive(Clone, Copy, PartialEq)]
pub struct Shown(pub State<Option<Shot>>);

/// `png`'s size in pixels, from its header alone.
pub(super) fn dims(png: &[u8]) -> Option<(f32, f32)> {
    let reader = image::ImageReader::new(std::io::Cursor::new(png)).with_guessed_format();
    let (w, h) = reader.ok()?.into_dimensions().ok()?;
    Some((w as f32, h.max(1) as f32))
}

/// The width for `png` at [`THUMB_H`].
fn thumb_width(png: &[u8]) -> f32 {
    dims(png).map_or(160.0, |(w, h)| {
        (THUMB_H * w / h).clamp(THUMB_W.0, THUMB_W.1)
    })
}

/// The dark disc on an image's top-right corner: the button reads on any
/// picture.
fn on_corner(button: impl IntoElement) -> Rect {
    rect()
        .position(Position::new_absolute().top(3.0).right(3.0))
        .corner_radius(99.0)
        .background(Color::from_argb(170, 0, 0, 0))
        .color(Color::WHITE)
        .child(button)
}

/// Attached and stored images, as thumbnails; on their top-right corner,
/// attached ones (`removable`) have a Remove, stored ones a Copy.
pub fn thumbs(
    p: &Palette,
    shots: &[Shot],
    (panel, removable): (&Arc<Panel>, bool),
    Shown(shown): Shown,
) -> Option<Element> {
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
        let (mut shown, this) = (shown, s.clone());
        let image = rect()
            .expanded()
            .on_press(move |_| shown.set(Some(this.clone())))
            .child(image);
        let (panel, id, this) = (panel.clone(), s.id, s.clone());
        let corner = if removable {
            icon_button(p, icons::X, "Remove", true, move |_| panel.remove_image(id))
        } else {
            icon_button(p, icons::COPY, "Copy image", true, move |_| {
                panel.copy_shot(this.clone());
            })
        };
        rect()
            .key(s.id)
            .width(Size::px(thumb_width(&s.png)))
            .height(Size::px(THUMB_H))
            .child(image)
            .child(on_corner(corner))
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
