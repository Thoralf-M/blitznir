//! The chat's thinking wave (`[panel] thinking_effect`):
//! zap/wave's stream drawn over the answer while it is asked: it waits in
//! the answer's slot until the first word, then fades out where it is
//! ([`SPOKEN_SECS`]): following text that streams and reflows made it jump
//! and flicker. A cancel takes it at once.
//! Never over the text but for that trail; never a key or a click (a
//! canvas over the card, drawn after it). 30 frames a second while words
//! come, 10 while it waits, none once it is gone.

use super::answer::{Line, Selection};
use super::style::use_palette;
use crate::zap::wave::{self, Cache, Frame, Ink, Stream, Trail};
use blitznir_core::beam::ease;
use blitznir_core::wave::{
    CHAT_OUT_SECS, FOLLOW_LAG, Follow, GROW_SECS, Heads, LEAD, MOVING_FRAME, OLD_FADE_SECS, frame,
    strength,
};
use freya::engine::prelude::Canvas as SkCanvas;
use freya::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Fading in over this from the ask.
const IN_SECS: f32 = 0.4;
/// Fading out over this from the first word, where it waited.
const SPOKEN_SECS: f32 = 0.35;
/// Before the first word the tip waits this far into the answer's slot.
const SLOT: f32 = 150.0;
/// The tip leads the text by [`LEAD`]: this close to the right edge, it
/// waits at the start of the next line instead.
const ROOM: f32 = 90.0;

/// One ask's wave: its clock, where the text went, the tip's spring, the
/// line it is on and the last line's tip while that fades.
#[derive(Default)]
struct Run {
    start: Option<Instant>,
    end: Option<Instant>,
    heads: Heads,
    follow: Option<Follow>,
    row: Option<f32>,
    row_at: f32,
    old: Option<(Follow, f32)>,
    drawn: Option<Instant>,
    /// The chat's messages when it was asked: one more is the answer.
    asked: usize,
    /// The first word came then; the tip's target and line until then.
    spoken: Option<Instant>,
    held: Option<((f32, f32), f32)>,
}

/// The wave of the open chat, shared by the band and its card; `gone`
/// moves when it has faded out, so the band takes its canvas away.
#[derive(Clone)]
pub struct Wave {
    run: Rc<RefCell<Run>>,
    cache: Rc<RefCell<Cache>>,
    gone: State<u64>,
    /// The streaming card's bottom, in the window: the slot is its last line.
    bottom: Rc<Cell<f32>>,
}

impl Run {
    fn secs(&self, at: Instant) -> f32 {
        self.start
            .map_or(0.0, |s| at.saturating_duration_since(s).as_secs_f32())
    }

    fn faded(&self, now: Instant) -> bool {
        let since = |at: Instant| now.saturating_duration_since(at).as_secs_f32();
        self.start.is_none()
            || self.end.is_some_and(|e| since(e) >= CHAT_OUT_SECS)
            || self.spoken.is_some_and(|s| since(s) >= SPOKEN_SECS)
    }
}

/// The band's side, with the chat's `messages`: an answer streaming starts
/// the wave, its end fades it out on the answer (true: on the last card), a
/// cancel (the question taken back) ends it.
pub fn use_wave(on: bool, streaming: bool, messages: usize) -> (Wave, bool) {
    let gone = use_state(|| 0);
    let _ = gone.read(); // the band renders again when it is gone
    let wave = use_hook(|| Wave {
        run: Rc::default(),
        cache: Rc::default(),
        gone,
        bottom: Rc::default(),
    });
    use_provide_context(|| wave.clone());
    let now = Instant::now();
    let mut r = wave.run.borrow_mut();
    if on && streaming && (r.start.is_none() || r.end.is_some()) {
        *r = Run {
            start: Some(now),
            asked: messages,
            ..Run::default()
        };
    } else if !streaming && r.start.is_some() && r.end.is_none() {
        if messages > r.asked {
            r.end = Some(now);
        } else {
            *r = Run::default();
        }
    }
    let fading = !streaming && !r.faded(now);
    drop(r);
    (wave, fading)
}

impl Wave {
    /// The streaming card reports its place here.
    pub fn card_sized(&self) -> impl Fn(Event<SizedEventData>) + 'static {
        let bottom = self.bottom.clone();
        move |e| bottom.set(e.area.max_y())
    }

    /// The next frame after this long; None: it is gone.
    fn wait(&self) -> Option<Duration> {
        let r = self.run.borrow();
        let now = Instant::now();
        if r.faded(now) {
            return None;
        }
        let moved = r.heads.moved().map_or(0.0, |m| r.secs(now) - m);
        Some(if r.end.is_some() {
            MOVING_FRAME
        } else {
            frame(Duration::from_secs_f32(moved.max(0.0)))
        })
    }

    /// A frame onto the card's canvas: `size` its width and the card's
    /// bottom, `lines` its text, all in its own points.
    fn draw(&self, c: &SkCanvas, (w, h): (f32, f32), lines: &[Line], ink: Ink) {
        let now = Instant::now();
        let mut r = self.run.borrow_mut();
        if r.faded(now) {
            return;
        }
        let t = r.secs(now);
        let dt = r
            .drawn
            .map_or(0.0, |d| now.saturating_duration_since(d).as_secs_f32());
        r.drawn = Some(now);
        let since = |at: Instant| now.saturating_duration_since(at).as_secs_f32();
        if r.spoken.is_none() && lines.iter().any(|l| l.right > l.left) {
            r.spoken = Some(now);
        }
        let k = match (r.spoken, r.end) {
            (Some(s), _) => strength(t, IN_SECS, since(s), SPOKEN_SECS),
            (None, e) => strength(t, IN_SECS, e.map_or(-1.0, since), CHAT_OUT_SECS),
        };
        let (target, row) = match r.held.filter(|_| r.spoken.is_some()) {
            Some(held) => held,
            None => aim(&mut r.heads, t, w, h, lines),
        };
        r.held = Some((target, row));
        let streams = r.streams(t, target, row, dt, k);
        let text: Vec<[f32; 4]> = lines
            .iter()
            .map(|l| {
                [
                    l.left - 2.0,
                    l.top,
                    l.right - l.left + 4.0,
                    l.bottom - l.top,
                ]
            })
            .collect();
        let trail = r.spoken.is_none().then_some(Trail {
            heads: &r.heads,
            t,
            k,
        });
        let f = Frame {
            streams: &streams,
            text: &text,
            trail,
        };
        wave::draw(c, ink, &mut self.cache.borrow_mut(), &f);
    }
}

/// The x-height's middle of a line: where the stream runs through it.
fn middle(l: &Line) -> f32 {
    l.base - 0.38 * (l.base - l.top)
}

/// Where the tip goes at `t`, and the number of the line it is on: ahead
/// of the text's end by [`LEAD`] and the head's speed, or the slot (the
/// first line) before the first word.
fn aim(heads: &mut Heads, t: f32, w: f32, h: f32, lines: &[Line]) -> ((f32, f32), f32) {
    let Some(last) = lines.iter().rev().find(|l| l.right > l.left) else {
        return ((SLOT, h - 8.5), 0.0);
    };
    let row = lines.iter().filter(|l| l.right > l.left).count() as f32 - 1.0;
    let y = middle(last);
    heads.push(t, (last.right, y), last.right + row * w);
    let lead = LEAD + heads.speed(t) * FOLLOW_LAG;
    if last.right + ROOM > w {
        let next = y + (last.bottom - last.top);
        return ((-8.0 + lead, next), row + 1.0);
    }
    ((last.right + lead, y), row)
}

impl Run {
    /// The tip's spring a frame on; a new line: the stream there grows in
    /// from the left edge while the last line's fades where it was.
    fn streams(&mut self, t: f32, target: (f32, f32), row: f32, dt: f32, k: f32) -> Vec<Stream> {
        if self.row.is_some_and(|r| r != row)
            && let Some(f) = self.follow.take()
        {
            self.old = Some((f, t));
            self.row_at = t;
        }
        self.row = Some(row);
        let f = self.follow.get_or_insert_with(|| Follow::at(target));
        f.step(target, dt);
        let at = |xb: f32, y: f32, k: f32| Stream {
            t,
            xa: -6.0,
            xb,
            y,
            k,
            core: 1.0,
        };
        let grow = if self.row_at > 0.0 {
            ease((t - self.row_at) / GROW_SECS)
        } else {
            1.0
        };
        let mut out = vec![at(4.0 + (f.x - 4.0) * grow, f.y, k)];
        if let Some((o, since)) = self.old {
            let fade = 1.0 - ease((t - since) / OLD_FADE_SECS);
            if fade > 0.0 {
                out.push(at(o.x, o.y, k * fade));
            } else {
                self.old = None;
            }
        }
        out
    }
}

/// The wave's canvas over an answer's card, its text pieces from `base`.
#[derive(PartialEq)]
pub struct Thinking {
    pub base: usize,
}

impl Component for Thinking {
    fn render(&self) -> impl IntoElement {
        let wave = use_consume::<Wave>();
        let sel = use_consume::<Selection>();
        let ink = if use_palette().dark {
            Ink::Light
        } else {
            Ink::Dark
        };
        let origin = use_hook(|| Rc::new(Cell::new((0.0f32, 0.0f32))));
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
        let (at, base) = (origin.clone(), self.base);
        let render = RenderCallback::new(move |ctx| {
            let (ox, oy) = at.get();
            let lines: Vec<Line> = (sel.lines(base).iter())
                .map(|l| Line {
                    left: l.left - ox,
                    right: l.right - ox,
                    top: l.top - oy,
                    base: l.base - oy,
                    bottom: l.bottom - oy,
                })
                .collect();
            let size = (ctx.size.width, wave.bottom.get() - oy);
            if let Some(c) = super::canvas(ctx) {
                wave.draw(c, size, &lines, ink);
            }
        });
        rect()
            .background(render)
            .position(Position::new_absolute())
            .width(Size::fill())
            .height(Size::fill())
            .on_sized(move |e: Event<SizedEventData>| {
                origin.set((e.area.min_x(), e.area.min_y()));
            })
    }
}
