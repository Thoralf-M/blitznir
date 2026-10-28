//! The header's empty parts: a drag moves the window, a double click zooms
//! it. On macOS the window moves its own frame there in steps (core's
//! zoom), each laid out and drawn at its size: AppKit's zoom stretched a
//! picture of the window and laid it out only at the end. Linux keeps
//! Freya's maximize (Wayland places no window by request).

use blitznir_core::platform::Rect as Frame;
use freya::prelude::*;

#[derive(Clone, Copy, PartialEq)]
struct ZoomGesture;

/// `area` moves the window when dragged and zooms it on a double click;
/// `before` is where a zoomed window goes back to.
pub fn zoom_drag(area: Rect, before: State<Option<Frame>>) -> Rect {
    if cfg!(not(target_os = "macos")) {
        return area.window_drag();
    }
    area.on_pointer_down(move |e: Event<PointerEventData>| {
        if e.button() != Some(MouseButton::Left) {
            return;
        }
        if EventsCombos::<ZoomGesture>::pressed(e.global_location()).is_double() {
            zoom(before);
        }
    })
    .on_global_pointer_move(|e: Event<PointerEventData>| {
        if EventsCombos::<ZoomGesture>::moved(e.global_location()) {
            Platform::get().with_window(Platform::window_id(), |w| {
                let _ = w.drag_window();
            });
        }
    })
    .on_global_pointer_up(|_: Event<PointerEventData>| {
        EventsCombos::<ZoomGesture>::released();
    })
}

/// The window's frame (points, outer) and the screen's free area.
#[cfg(target_os = "macos")]
fn frames(w: &freya::winit::window::Window) -> Option<(Frame, Frame)> {
    use freya::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let s = w.scale_factor();
    let at = w.outer_position().ok()?.to_logical::<f64>(s);
    let size = w.outer_size().to_logical::<f64>(s);
    let now = Frame {
        x: at.x,
        y: at.y,
        w: size.width,
        h: size.height,
    };
    let RawWindowHandle::AppKit(appkit) = w.window_handle().ok()?.as_raw() else {
        return None;
    };
    Some((now, blitznir::platform::visible_frame(appkit.ns_view)?))
}

#[cfg(not(target_os = "macos"))]
fn frames(_: &freya::winit::window::Window) -> Option<(Frame, Frame)> {
    None
}

/// Put the window at `r` (outer frame, points), moved and sized at once.
#[cfg(target_os = "macos")]
fn place(w: &mut freya::winit::window::Window, r: Frame) {
    use freya::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(h) = w.window_handle()
        && let RawWindowHandle::AppKit(appkit) = h.as_raw()
    {
        blitznir::platform::set_frame(appkit.ns_view, r);
    }
}

#[cfg(not(target_os = "macos"))]
fn place(_: &mut freya::winit::window::Window, _: Frame) {}

/// Zoom the window: fill the free area, or go back; frame by frame.
fn zoom(mut before: State<Option<Frame>>) {
    let platform = Platform::get();
    let id = Platform::window_id();
    let read = std::rc::Rc::new(std::cell::Cell::new(None));
    let into = read.clone();
    platform.with_window(id, move |w| into.set(Some(frames(w))));
    spawn(async move {
        // with_window runs on the next turn of the loop
        let mut tries = 0;
        let (from, free) = loop {
            match read.take() {
                Some(Some(f)) => break f,
                Some(None) => return, // no screen to fill
                None => {}
            }
            tries += 1;
            if tries > 50 {
                return;
            }
            timer(std::time::Duration::from_millis(2)).await;
        };
        let (to, back) = blitznir_core::zoom::target(from, free, *before.peek());
        before.set(back);
        let start = std::time::Instant::now();
        loop {
            let t = start.elapsed().as_secs_f32() / blitznir_core::zoom::SECS;
            let r = blitznir_core::zoom::between(from, to, blitznir_core::beam::ease(t));
            platform.with_window(id, move |w| place(w, r));
            if t >= 1.0 {
                break;
            }
            timer(std::time::Duration::from_millis(16)).await;
        }
    });
}
