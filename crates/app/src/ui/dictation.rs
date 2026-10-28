//! The dictation wave in our own composer (`[panel] dictation_effect`):
//! the overlay over other apps (zap/caret.rs) asks the app in
//! front where its caret is, and that is never us. So while a dictation
//! records and the panel has key focus (it types into the composer), the
//! same wave is drawn here, 5 points under the composer's last line, its
//! tip at the text's end, in the panel's ink. Freya's Input keeps its
//! laid-out paragraph and cursor to itself; the dictation types at the end,
//! so the end is taken from the composer's hidden copy of its text, laid
//! out as the input lays it out. A canvas over the field: never a key or a
//! click, never over the text. 30 frames a second while words come, 10
//! when they rest, none once it has faded out.

use super::style::use_palette;
use crate::zap::caret::Scene;
use crate::zap::caret::window::{Tip, draw};
use crate::zap::wave::{Cache, Ink};
use blitznir_core::platform::Rect;
use blitznir_core::wave::{MOVING_FRAME, Spot, UNDER, frame};
use freya::engine::prelude::{Canvas as SkCanvas, ClipOp, RectHeightStyle, RectWidthStyle, SkRect};
use freya::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The input's own padding round its text (freya's input layout).
pub const PAD: f32 = 8.0;

/// One recording's wave: its clock and place, the tip's spring, the glows.
#[derive(Default)]
struct Run {
    scene: Option<Scene>,
    tip: Tip,
    cache: Cache,
}

/// The composer's wave, shared by the composer and its canvas: `gone`
/// moves once it has faded out, so the composer takes the canvas away.
#[derive(Clone)]
pub struct Wave {
    run: Rc<RefCell<Run>>,
    gone: State<u64>,
    /// The composer's text laid out as the input lays it out, with one
    /// space after it ([`super::composer`]'s measure), and the text's
    /// length in UTF-16: that space is where the caret is.
    pub holder: ParagraphHolder,
    pub end: Rc<Cell<usize>>,
}

/// The composer's side: a wave while `active` (the effect on, a dictation
/// recording, the panel focused); true while it shows or fades out.
pub fn use_wave(active: bool) -> (Wave, bool) {
    let gone = use_state(|| 0);
    let _ = gone.read(); // the composer renders again when it is gone
    let wave = use_hook(|| Wave {
        run: Rc::default(),
        gone,
        holder: ParagraphHolder::default(),
        end: Rc::default(),
    });
    use_provide_context(|| wave.clone());
    let now = Instant::now();
    let mut r = wave.run.borrow_mut();
    let live = r.scene.is_some_and(|s| s.end.is_none());
    if active && !live {
        let id = r.scene.map_or(1, |s| s.id + 1);
        *r = Run {
            scene: Some(Scene {
                id,
                start: now,
                end: None,
                spot: None,
                frame: Rect::default(),
                moved: now,
            }),
            ..Run::default()
        };
    } else if !active && let Some(s) = r.scene.as_mut().filter(|s| s.end.is_none()) {
        s.end = Some(now);
    }
    let shows = r.scene.is_some_and(|s| !s.gone(now));
    drop(r);
    (wave, shows)
}

impl Wave {
    /// The next frame after this long; None: it is gone.
    fn wait(&self) -> Option<Duration> {
        let now = Instant::now();
        let s = self.run.borrow().scene.filter(|s| !s.gone(now))?;
        Some(if s.end.is_some() {
            MOVING_FRAME
        } else {
            frame(now.saturating_duration_since(s.moved))
        })
    }

    /// Where the caret is, in the field's points: under the input's
    /// padding, scrolled as the input scrolls to show its end.
    fn spot(&self, (w, h): (f32, f32)) -> Option<Spot> {
        let laid = self.holder.0.borrow();
        let laid = laid.as_ref()?;
        let p = &laid.paragraph;
        let s = laid.scale_factor as f32;
        let end = self.end.get();
        let space =
            p.get_rects_for_range(end..end + 1, RectHeightStyle::Tight, RectWidthStyle::Tight);
        let at = space.first()?.rect;
        let mid = (at.top + at.bottom) / 2.0;
        let lines = p.get_line_metrics();
        let line = (lines.iter())
            .find(|m| mid < (m.baseline + m.descent) as f32)
            .or(lines.last())?;
        let base = line.baseline as f32 / s;
        let bottom = base + line.descent as f32 / s;
        let scrolled = (bottom + PAD - h).max(0.0);
        let at_y = |v: f32| f64::from(PAD + v - scrolled);
        let x = |v: f32| f64::from(PAD + v / s);
        Some(Spot {
            frame: Rect {
                x: 0.0,
                y: 0.0,
                w: f64::from(w),
                h: f64::from(h),
            },
            y: at_y(base) + UNDER,
            left: x(line.left as f32) - 6.0,
            caret: x(at.left),
            text_bottom: at_y(bottom),
        })
    }

    /// A frame onto the field's canvas, `size` its size.
    fn draw(&self, c: &SkCanvas, size: (f32, f32), ink: Ink) {
        let now = Instant::now();
        let spot = self.spot(size);
        let mut r = self.run.borrow_mut();
        let Run { scene, tip, cache } = &mut *r;
        let Some(s) = scene.as_mut().filter(|s| !s.gone(now)) else {
            return;
        };
        if spot.map(|a| (a.caret, a.y)) != s.spot.map(|a| (a.caret, a.y)) {
            s.moved = now;
        }
        s.spot = spot.or(s.spot);
        let s = *s;
        c.save();
        c.clip_rect(SkRect::from_wh(size.0, size.1), ClipOp::Intersect, true);
        draw(c, &s, now, tip, cache, ink);
        c.restore();
    }
}

/// The wave's canvas over the composer's field.
#[derive(PartialEq)]
pub struct Dictation;

impl Component for Dictation {
    fn render(&self) -> impl IntoElement {
        let wave = use_consume::<Wave>();
        let ink = if use_palette().dark {
            Ink::Light
        } else {
            Ink::Dark
        };
        let pace = wave.clone();
        use_hook(move || {
            let window = Platform::get().current_window();
            spawn(async move {
                while let Some(wait) = pace.wait() {
                    window.request_redraw();
                    timer(wait).await;
                }
                let mut gone = pace.gone;
                gone.with_mut(|mut n| *n += 1);
            });
        });
        let render = RenderCallback::new(move |ctx| {
            let size = (ctx.size.width, ctx.size.height);
            if let Some(c) = super::canvas(ctx) {
                wave.draw(c, size, ink);
            }
        });
        // at the field's corner, not after the input; a percentage, as a
        // fill takes only the room the input leaves
        rect()
            .background(render)
            .position(Position::new_absolute().top(0.0).left(0.0))
            .width(Size::percent(100.0))
            .height(Size::percent(100.0))
    }
}
