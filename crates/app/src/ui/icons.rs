//! The icons: hand-drawn 24×24 strokes, drawn in the
//! colour they are given, always with a tooltip.

use super::{
    style::{Look, Palette, button_if, pressable},
    tip::Tip,
};
use freya::prelude::*;

macro_rules! svg {
    ($($body:literal)+) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" "#,
            r#"stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">"#,
            $($body,)+
            "</svg>"
        )
        .as_bytes()
    };
}

pub const SIDEBAR: &[u8] =
    svg!(r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16"/>"#);
pub const NEW: &[u8] = svg!(
    r#"<path d="M12 4H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-6"/>"#
    r#"<path d="M17.5 3.5a2.1 2.1 0 0 1 3 3L12 15l-4 1 1-4z"/>"#
);
/// Six wide teeth: at 16 px on a 1x screen, eight thin ones blurred into a
/// round dot.
pub const GEAR: &[u8] = svg!(
    r#"<path d="M9.78 4.73 9.46 1.81A10.5 10.5 0 0 1 14.54 1.81L14.22 4.73A7.6 7.6 0 0 1 17.18 6.44L19.55 4.71A10.5 10.5 0 0 1 22.09 9.11L19.41 10.29A7.6 7.6 0 0 1 19.41 13.71L22.09 14.89A10.5 10.5 0 0 1 19.55 19.29L17.18 17.56A7.6 7.6 0 0 1 14.22 19.27L14.54 22.19A10.5 10.5 0 0 1 9.46 22.19L9.78 19.27A7.6 7.6 0 0 1 6.82 17.56L4.45 19.29A10.5 10.5 0 0 1 1.91 14.89L4.59 13.71A7.6 7.6 0 0 1 4.59 10.29L1.91 9.11A10.5 10.5 0 0 1 4.45 4.71L6.82 6.44A7.6 7.6 0 0 1 9.78 4.73z"/>"#
    r#"<circle cx="12" cy="12" r="3"/>"#
);
pub const CAMERA: &[u8] = svg!(
    r#"<path d="M3 9a2 2 0 0 1 2-2h2l2-3h6l2 3h2a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"#
    r#"<circle cx="12" cy="13" r="3.5"/>"#
);
pub const IMAGE: &[u8] = svg!(
    r#"<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="1.8"/>"#
    r#"<path d="M21 15.5 16 11l-9 9"/>"#
);
pub const COPY: &[u8] = svg!(
    r#"<rect x="9" y="9" width="11" height="11" rx="2"/>"#
    r#"<path d="M15 9V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h3"/>"#
);
// Write out: an arrow into a text caret, "put this text at the cursor".
pub const OUT: &[u8] = svg!(
    r#"<path d="M3 12h11M10 8l4 4-4 4"/>"#
    r#"<path d="M19 5v14M17 5h4M17 19h4"/>"#
);
pub const TRASH: &[u8] = svg!(
    r#"<path d="M4 7h16M9 7V4h6v3M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M10 11v6M14 11v6"/>"#
);
// Retry: a circling arrow, "ask again".
pub const RETRY: &[u8] = svg!(r#"<path d="M20 12a8 8 0 1 1-2.3-5.7"/><path d="M20 4v5h-5"/>"#);
pub const CHAT: &[u8] = svg!(r#"<path d="M4 5h16v11H10l-5 4v-4H4z"/>"#);
pub const SEARCH: &[u8] = svg!(r#"<circle cx="11" cy="11" r="6"/><path d="m20 20-4.5-4.5"/>"#);
pub const EYE: &[u8] = svg!(
    r#"<path d="M2 12s3.6-6.5 10-6.5S22 12 22 12s-3.6 6.5-10 6.5S2 12 2 12z"/>"#
    r#"<circle cx="12" cy="12" r="3"/>"#
);
pub const PENCIL: &[u8] = svg!(r#"<path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M13 7l4 4"/>"#);
// Suggest: a wand and its sparks.
pub const WAND: &[u8] =
    svg!(r#"<path d="M4 20 14 10"/><path d="M15 3v3M18.5 5.5l-2 2M21 9h-3M13 4.5 14 6"/>"#);
pub const PLUS: &[u8] = svg!(r#"<path d="M12 5v14M5 12h14"/>"#);
// Open a link: an arrow out of a box.
pub const LINK: &[u8] = svg!(
    r#"<path d="M14 4h6v6M20 4l-9 9"/>"#
    r#"<path d="M18 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4"/>"#
);
pub const X: &[u8] = svg!(r#"<path d="M6 6l12 12M18 6 6 18"/>"#);
// Thinking on: a bulb with its rays; off: the bulb struck through.
pub const THINK: &[u8] = svg!(
    r#"<path d="M9.5 19h5M10.5 21.5h3"/>"#
    r#"<path d="M12 6.5a5 5 0 0 0-3 9c.6.5 1 1.2 1 2v.5h4v-.5c0-.8.4-1.5 1-2a5 5 0 0 0-3-9z"/>"#
    r#"<path d="M12 1.5v2M5 4.5l1.4 1.4M19 4.5l-1.4 1.4M2.5 11.5h2M19.5 11.5h2"/>"#
);
pub const THINK_OFF: &[u8] = svg!(
    r#"<path d="M9.5 19h5M10.5 21.5h3"/>"#
    r#"<path d="M12 6.5a5 5 0 0 0-3 9c.6.5 1 1.2 1 2v.5h4v-.5c0-.8.4-1.5 1-2a5 5 0 0 0-3-9z"/>"#
    r#"<path d="M4 3.5 20 21"/>"#
);
pub const BACK: &[u8] = svg!(r#"<path d="M19 12H5M11 18l-6-6 6-6"/>"#);
// A sidebar section shown, and folded away.
pub const OPEN: &[u8] = svg!(r#"<path d="M7 10l5 5 5-5"/>"#);
pub const SHUT: &[u8] = svg!(r#"<path d="M10 7l5 5-5 5"/>"#);

pub fn icon(src: &'static [u8], color: Color, size: f32) -> SvgViewer {
    SvgViewer::new(src)
        .color(color)
        .show_loader(false)
        .width(Size::px(size))
        .height(Size::px(size))
}

/// A borderless icon button with its tooltip; `small` in lists and message rows.
pub fn icon_button(
    p: &Palette,
    src: &'static [u8],
    tip: impl Into<String>,
    small: bool,
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> Tip {
    icon_button_if(p, src, tip, (small, true), on_press)
}

/// [`icon_button`], faded and inert while `enabled` is false.
pub fn icon_button_if(
    p: &Palette,
    src: &'static [u8],
    tip: impl Into<String>,
    (small, enabled): (bool, bool),
    on_press: impl FnMut(Event<PressEventData>) + 'static,
) -> Tip {
    let (size, pad) = if small {
        (14.0, (2.0, 3.0))
    } else {
        (16.0, (4.0, 5.0))
    };
    let color = if enabled {
        p.muted
    } else {
        Color::lerp(p.muted, p.bg, 0.55)
    };
    let b = button_if(p, Look::Tool, enabled)
        .padding(pad)
        .child(icon(src, color, size));
    let b = pressable(b, on_press);
    Tip::new(tip).child(b)
}
