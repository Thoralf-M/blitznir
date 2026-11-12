//! An image shown large over the window, as attached (not as downscaled for
//! the model). It opens fitted, never past its own size, and a fitted one
//! follows the window's size; Fit, 1:1, − and +
//! zoom it, the wheel zooms where the pointer is, a drag moves a zoomed one.
//! Copy copies it; Close, a press beside it or Esc closes it. The geometry
//! is core's picture.rs.

use super::{
    section::choice,
    style::{Look, Palette, use_palette},
    thumbs::{Shown, dims},
};
use crate::panel::{Panel, view::Shot};
use blitznir_core::picture::{MAX_ZOOM, Picture};
use freya::prelude::*;
use std::sync::Arc;

/// Scroll to zoom: a factor of e for this many points of it.
const WHEEL: f32 = 400.0;
/// − and +: one step.
const STEP: f32 = 1.25;
/// The bar's gap to the window's top and to the image.
const GAP: f32 = 6.0;
/// The strip at the top the bar sits in (it is 22 tall): the image lives
/// below it, so a fitted one is never under the bar.
const BAR: f32 = 22.0 + 2.0 * GAP;

/// The viewer while an image is shown.
pub fn large(shown: Shown) -> Option<Element> {
    let shot = shown.0.read().clone()?;
    Some(Viewer { shot, shown }.into_element())
}

#[derive(PartialEq)]
struct Viewer {
    shot: Shot,
    shown: Shown,
}

/// The image's own size in points: its pixels are the screen's, half as
/// many points on a Retina screen.
fn own_size(png: &[u8], window: (f32, f32)) -> (f32, f32) {
    let scale = *Platform::get().current_window().scale_factor.read() as f32;
    dims(png).map_or(window, |(w, h)| (w / scale, h / scale))
}

/// The bar over the image, centred at its top: the zoom buttons, its
/// percent of 1:1, Copy, Close.
fn bar(p: &Palette, mut view: State<Picture>, copy: impl Fn() + 'static, mut shown: Shown) -> Rect {
    let pic = *view.read();
    let button = |text: &str, f: Box<dyn Fn(Picture) -> Picture>| {
        choice(p, Look::Tool, text, move |e| {
            e.stop_propagation(); // the press around it closes the view
            let next = f(*view.peek());
            view.set(next);
        })
    };
    let close = choice(p, Look::Tool, "Close", move |e| {
        e.stop_propagation();
        shown.0.set(None);
    });
    let copy = choice(p, Look::Tool, "Copy", move |e| {
        e.stop_propagation();
        copy();
    });
    let bar = rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(4.0)
        .padding((4.0, 6.0))
        .corner_radius(8.0)
        .background(p.bg)
        .on_press(|e: Event<PressEventData>| e.stop_propagation()) // not beside the image
        .color(p.fg)
        .child(button("Fit", Box::new(|v| v.fitted(MAX_ZOOM))))
        .child(button("1:1", Box::new(|v| v.zoomed(1.0))))
        .child(button("−", Box::new(|v| v.zoomed(v.zoom / STEP))))
        .child(
            label()
                .width(Size::px(44.0))
                .text(format!("{:.0}%", pic.zoom * 100.0))
                .font_size(11.0)
                .text_align(TextAlign::Center),
        )
        .child(button("+", Box::new(|v| v.zoomed(v.zoom * STEP))))
        .child(copy)
        .child(close);
    // a layer above the image: on the same one, an image decoded after the
    // bar was mounted painted over the bar's background
    rect()
        .layer(Layer::Relative(4))
        .position(Position::new_absolute().top(GAP).left(0.0))
        .cross_align(Alignment::Center)
        .child(bar)
}

/// The room under the bar's strip, clipping what is zoomed past it.
fn below_bar(room: (f32, f32), image: Rect) -> Rect {
    rect()
        .position(Position::new_absolute().left(0.0).top(BAR))
        .width(Size::px(room.0))
        .height(Size::px(room.1))
        .overflow(Overflow::Clip)
        .child(image)
}

impl Component for Viewer {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let size = super::window_size();
        let window = (size.width, size.height);
        let room = (window.0, (window.1 - BAR).max(1.0));
        // as attached: a stored image's original is read once, as it opens
        let shot = self.shot.clone();
        let png = use_hook(|| panel.largest(&shot));
        let image_size = own_size(&png, window);
        let mut view = use_state(|| Picture::new(image_size, room));
        if view.peek().window != room {
            let resized = view.peek().in_window(room);
            view.set(resized);
        }
        // the pointer where a drag began, and the pan then
        let mut drag = use_state(|| None::<(CursorPoint, (f32, f32))>);
        // a press begun on the image: its release beside it closes nothing
        let mut grabbed = use_state(|| false);
        let pic = *view.read();
        let (x, y) = pic.origin();
        let (w, h) = pic.shown();
        // its own key: the thumbnail's decoded image is cached under the id;
        // decoded once as it is: decoded to its laid-out size, each zoom
        // step decoded it anew and showed a spinner meanwhile (the flicker)
        let image = ImageViewer::new(((self.shot.id, "full"), png.clone()))
            .decode_mode(DecodeMode::Source)
            .width(Size::px(w))
            .height(Size::px(h));
        let image = rect()
            .position(Position::new_absolute().left(x).top(y))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                grabbed.set(false);
            })
            .on_pointer_down(move |e: Event<PointerEventData>| {
                e.stop_propagation(); // the backdrop's would let it go
                if e.data().is_primary() {
                    drag.set(Some((e.global_location(), view.peek().pan)));
                    grabbed.set(true);
                }
            })
            .child(image);
        let image = below_bar(room, image);
        let shot = self.shot.clone();
        let copy = move || panel.copy_shot(shot.clone());
        let mut shown = self.shown;
        let bar = bar(&p, view, copy, shown).width(Size::px(window.0));
        rect()
            .position(Position::new_global().top(0.0).left(0.0))
            .width(Size::px(window.0))
            .height(Size::px(window.1))
            .overflow(Overflow::Clip)
            .background(Color::from_argb(200, 0, 0, 0))
            .layer(Layer::Overlay)
            .on_pointer_down(move |_| grabbed.set(false))
            .on_press(move |_| {
                if *grabbed.peek() {
                    grabbed.set(false);
                } else {
                    shown.0.set(None);
                }
            })
            .on_wheel(move |e: Event<WheelEventData>| {
                let d = e.data();
                let at = d.global_location;
                let factor = (d.delta_y as f32 / WHEEL).exp();
                let next = view
                    .peek()
                    .zoomed_at(view.peek().zoom * factor, (at.x as f32, at.y as f32 - BAR));
                view.set(next);
            })
            .on_global_pointer_move(move |e: Event<PointerEventData>| {
                if let Some((from, pan)) = *drag.peek() {
                    let by = e.global_location() - from;
                    let next = view.peek().panned(pan, (by.x as f32, by.y as f32));
                    view.set(next);
                }
            })
            .on_global_pointer_up(move |_| {
                if drag.peek().is_some() {
                    drag.set(None);
                }
            })
            .child(image)
            .child(bar)
    }

    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.shot.id) // another image opens fitted
    }
}
