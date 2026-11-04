//! Images as thumbnails: attached to the composer (with a Remove) and in
//! a chat's messages. A press on one shows it large over the window, fitted
//! and never past its own size; a press or Esc closes it.

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

/// The room the large image leaves around itself, in points.
const MARGIN: f32 = 24.0;

/// The image shown large, if any: the window's (`app` provides it).
#[derive(Clone, Copy)]
pub struct Shown(pub State<Option<Shot>>);

/// `png`'s size in pixels, from its header alone.
fn dims(png: &[u8]) -> Option<(f32, f32)> {
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

/// Attached and stored images, as thumbnails; attached ones have a Remove
/// on their top-right corner.
pub fn thumbs(
    p: &Palette,
    shots: &[Shot],
    remove: Option<&Arc<Panel>>,
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

/// The image shown large over the whole window, if one is.
pub fn shown_large(Shown(mut shown): Shown) -> Option<Element> {
    let s = shown.read().clone()?;
    let window = super::window_size();
    // its pixels are the screen's: half as many points on a Retina screen
    let scale = *Platform::get().current_window().scale_factor.read() as f32;
    let (w, h) = dims(&s.png).map_or((window.width, window.height), |(w, h)| {
        (w / scale, h / scale)
    });
    let fit = ((window.width - 2.0 * MARGIN) / w)
        .min((window.height - 2.0 * MARGIN) / h)
        .min(1.0);
    let image = ImageViewer::new((s.id, s.png.clone()))
        .width(Size::px(w * fit))
        .height(Size::px(h * fit));
    let backdrop = rect()
        .position(Position::new_global().top(0.0).left(0.0))
        .width(Size::px(window.width))
        .height(Size::px(window.height))
        .main_align(Alignment::Center)
        .cross_align(Alignment::Center)
        .background(Color::from_argb(200, 0, 0, 0))
        .layer(Layer::Overlay)
        .on_press(move |_| shown.set(None))
        .child(image);
    Some(backdrop.into_element())
}
