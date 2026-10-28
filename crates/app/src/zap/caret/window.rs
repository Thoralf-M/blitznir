//! The dictation wave's window: line-sized, transparent, click-through,
//! always on top, never active; one canvas that draws the wave in light
//! only (it brightens the app below, never darkens it: the dual ink's dark
//! halo read as black specks and dark rims; on a white page it shows
//! little) at 30 frames a second while the caret moves, 10 when it rests,
//! none once it has faded out.

use super::super::wave::{self, Cache, Frame, Ink, Stream};
use super::{Caret, Scene};
use blitznir_core::beam::smooth;
use blitznir_core::platform::Rect;
use blitznir_core::wave::{
    CARET_OUT_SECS, Follow, MOVING_FRAME, PAST_CARET, Spot, frame, strength,
};
use freya::engine::prelude::Canvas as SkCanvas;
use freya::prelude::*;
use freya::winit::window::{WindowAttributes, WindowLevel};
use std::sync::Arc;
use std::time::Instant;

/// The tip's core light, a bit brighter than the base wave's, tuned by eye.
const CORE: f32 = 1.35;
/// The dictation wave runs at 0.9 of full strength, so it never outshines the text.
const STRENGTH: f32 = 0.9;
/// It fades in over this, and away over [`CARET_OUT_SECS`] when the app
/// stops saying where its caret is.
const IN_SECS: f32 = 0.35;

pub fn config(caret: Arc<Caret>, id: u64, at: Rect) -> WindowConfig {
    WindowConfig::new(move || app(caret.clone(), id))
        .with_title("Blitznir dictation")
        .with_app_id(crate::APP_ID)
        .with_size(at.w, at.h)
        .with_decorations(false)
        .with_transparency(true)
        .with_background(Color::TRANSPARENT)
        .with_resizable(false)
        .with_window_attributes(move |attrs, _| place(attrs, at))
        .with_window_handle(|w| {
            // clicks go to the app below
            if let Err(e) = w.set_cursor_hittest(false) {
                log::warn!("dictation wave: the window takes clicks: {e}");
            }
        })
}

/// On top, not active: the app being dictated into keeps the keyboard.
fn place(attrs: WindowAttributes, at: Rect) -> WindowAttributes {
    let attrs = attrs
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_active(false);
    #[cfg(target_os = "macos")]
    {
        use freya::winit::dpi::LogicalPosition;
        use freya::winit::platform::macos::WindowAttributesExtMacOS;
        attrs
            .with_position(LogicalPosition::new(at.x, at.y))
            .with_has_shadow(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = at;
        attrs
    }
}

/// What the canvas keeps between frames: the tip's spring, the last place,
/// how much of the wave shows.
#[derive(Default)]
pub struct Tip {
    follow: Option<Follow>,
    last: Option<Spot>,
    shown: f32,
    at: Option<Instant>,
}

/// Draw session `s` at `now` in `ink` onto a canvas whose origin is
/// `s.frame`'s (the window's; the composer's in the panel, ui/dictation.rs).
pub fn draw(c: &SkCanvas, s: &Scene, now: Instant, tip: &mut Tip, cache: &mut Cache, ink: Ink) {
    let dt = tip
        .at
        .map_or(0.0, |a| now.saturating_duration_since(a).as_secs_f32());
    tip.at = Some(now);
    let want = if s.spot.is_some() { 1.0 } else { 0.0 };
    tip.shown += (want - tip.shown).clamp(-dt / CARET_OUT_SECS, dt / IN_SECS);
    let Some(spot) = s.spot.or(tip.last) else {
        return;
    };
    let target = ((spot.caret as f32) + PAST_CARET, spot.y as f32);
    let f = tip.follow.get_or_insert_with(|| Follow::at(target));
    if tip.last.is_some_and(|l| l.frame.y != spot.frame.y) {
        f.drop_to(target.1); // another line: the window moved with it
    }
    f.step(target, dt);
    tip.last = Some(spot);
    let run = now.saturating_duration_since(s.start).as_secs_f32();
    let after = s
        .end
        .map_or(-1.0, |e| now.saturating_duration_since(e).as_secs_f32());
    let k = strength(run, IN_SECS, after, CARET_OUT_SECS) * STRENGTH * smooth(tip.shown);
    let (ox, oy) = (s.frame.x as f32, s.frame.y as f32);
    let streams = [Stream {
        t: run,
        xa: spot.left as f32 - ox,
        xb: f.x - ox,
        y: f.y - oy,
        k,
        core: CORE,
    }];
    // the line left of the caret, down to the text's bottom
    let right = spot.caret as f32 - ox + 1.0;
    let bottom = spot.text_bottom as f32 - oy;
    let text = [[-100.0, -100.0, right + 100.0, bottom + 100.0]];
    let f = Frame {
        streams: &streams,
        text: &text,
        trail: None,
    };
    wave::draw(c, ink, cache, &f);
}

fn app(caret: Arc<Caret>, id: u64) -> impl IntoElement {
    let scene = caret.clone();
    use_hook(move || {
        let window = Platform::get().current_window();
        spawn(async move {
            loop {
                window.request_redraw();
                let now = Instant::now();
                let Some(s) = scene.scene().filter(|s| s.id == id && !s.gone(now)) else {
                    break;
                };
                let wait = if s.end.is_some() {
                    MOVING_FRAME
                } else {
                    frame(now.saturating_duration_since(s.moved))
                };
                timer(wait).await;
            }
        });
    });
    let (mut tip, mut cache) = (Tip::default(), Cache::default());
    let render = RenderCallback::new(move |ctx| {
        let now = Instant::now();
        let scene = caret.scene().filter(|s| s.id == id && !s.gone(now));
        if let (Some(s), Some(c)) = (scene, crate::ui::canvas(ctx)) {
            draw(c, &s, now, &mut tip, &mut cache, Ink::Light);
        }
    });
    rect()
        .expanded()
        .child(rect().background(render).expanded())
}
