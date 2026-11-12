//! One frame of the beam, a pure function of the time since it started
//! and ended (core's beam): the thread under the menu bar, the column in
//! six layers round a wide soft core, ragged particle fronts where it drops
//! and where it draws down, and the motes and the flare ([`super::motes`]).
//! It never draws over the selected text: that rect is clipped out, and the
//! flare's light lies only above the line's top edge.

use super::{
    Beam, Pt, motes,
    paint::{Buffers, Pen, Stops, c4, radial},
};
use blitznir_core::beam::{Phase, h2, noise1};
use freya::engine::prelude::{
    BlendMode, BlurStyle, Canvas, ClipOp, Colors, Gradient, Interpolation, MaskFilter, Paint,
    PathBuilder, SkRect, TileMode, shaders,
};
use std::time::Instant;

/// The frame clock runs 0.5 s ahead of the beam's start: the flickers and
/// wobbles were tuned by eye on that phase, and start mid-motion with it.
const PHASE_T0: f32 = 0.5;
/// The core's half width, about 80 points across in all, in screen points
/// (as the overlay's): a wide core, tuned by eye.
const CORE: f32 = 40.0;
/// The drop's and the collapse's ragged fronts stop the column this short.
const TAPER: f32 = 50.0;

/// What a frame knows about the beam.
pub struct Frame {
    /// The frame clock: seconds since the start, plus [`PHASE_T0`].
    pub t: f32,
    pub ph: Phase,
    /// Where it lands: 80 % along the selection, toward the hammer (which
    /// crosses the least of the text above), on the line's top edge.
    pub land: Pt,
    pub bar: f32,
}

/// The gradients made once, and the frames' buffers.
pub struct Cache {
    path: PathBuilder,
    bufs: Buffers,
    flare: [Paint; 6],
    flow: Paint,
}

impl Default for Cache {
    fn default() -> Cache {
        let violet = |a| c4(0.5, 0.4, 1.0, a);
        let flare: [&Stops; 6] = [
            &[
                (0.0, c4(1.0, 1.0, 1.0, 0.95)),
                (0.4, c4(0.75, 0.7, 1.0, 0.45)),
                (1.0, violet(0.0)),
            ],
            &[(0.0, c4(0.8, 0.7, 1.0, 0.3)), (1.0, violet(0.0))],
            &[
                (0.0, c4(1.0, 0.97, 1.0, 0.8)),
                (0.3, c4(0.72, 0.58, 1.0, 0.4)),
                (1.0, c4(0.5, 0.35, 1.0, 0.0)),
            ],
            &[
                (0.0, c4(1.0, 1.0, 1.0, 0.95)),
                (1.0, c4(0.9, 0.85, 1.0, 0.0)),
            ],
            &[(0.0, c4(1.0, 1.0, 1.0, 1.0)), (1.0, c4(1.0, 1.0, 1.0, 0.0))],
            &[(0.0, c4(0.6, 0.5, 1.0, 0.14)), (1.0, violet(0.0))],
        ];
        Cache {
            path: PathBuilder::new(),
            bufs: Buffers::default(),
            flare: flare.map(radial),
            flow: radial(&[
                (0.0, c4(0.9, 0.95, 1.0, 1.0)),
                (1.0, c4(0.6, 0.75, 1.0, 0.0)),
            ]),
        }
    }
}

/// The light's own flicker, a few per cent.
pub fn flicker(t: f32) -> f32 {
    1.0 + 0.05 * noise1(t * 21.0, 81) + 0.03 * noise1(t * 57.0, 82)
}

/// The beam at `now` onto `c`, in the overlay's points.
pub fn draw(c: &Canvas, beam: &Beam, now: Instant, cache: &mut Cache) {
    let ph = beam.phase(now);
    if ph.out <= 0.0 {
        return;
    }
    let [ax, ay, aw, ah] = beam.aim;
    let f = Frame {
        t: ph.run + PHASE_T0,
        ph,
        land: (ax + aw * super::LAND_AT as f32, ay),
        bar: beam.bar,
    };
    c.save();
    c.clip_rect(
        SkRect::new(ax, ay, ax + aw, ay + ah),
        ClipOp::Difference,
        true,
    );
    let Cache {
        path,
        bufs,
        flare,
        flow,
    } = cache;
    let mut pen = Pen::new(c, path);
    thread(&mut pen, &f, beam.src, &mut bufs.dots[0]);
    if ph.drop > 0.0 {
        column(&mut pen, &f, bufs, flow);
        c.save();
        c.clip_rect(SkRect::new(-1e4, -1e4, 1e4, ay), None, true);
        let k = ph.out * ph.flare;
        motes::flare(&mut pen, &f, k, flare, &mut bufs.dots[0]);
        c.restore();
    }
    c.restore();
}

/// The menu-bar thread: a hairline with a soft glow, specks running along.
fn thread(pen: &mut Pen, f: &Frame, src: Pt, specks: &mut Vec<(Pt, f32)>) {
    let (run, lx, y) = (f.ph.run, f.land.0, f.bar);
    let reach = f.ph.reach;
    let bar = [[(src.0, y), (src.0 + (lx - src.0) * reach, y)]];
    let a = (0.4 + 0.1 * noise1(run * 5.0, 9)) * f.ph.bar;
    pen.segs(&bar, 4.0, c4(0.3, 0.5, 1.0, 0.2 * a), 3.0);
    pen.segs(&bar, 0.5, c4(0.8, 0.9, 1.0, a), 0.0);
    specks.clear();
    specks.extend((0..12u32).map(|j| {
        let u = (run * (0.8 + 0.6 * h2(j, 1)) + h2(j, 2)).fract();
        let x = src.0 + (lx - src.0) * u * reach;
        ((x, y + (h2(j, 3) - 0.5) * 1.5), 0.45)
    }));
    pen.dots(specks, c4(0.9, 0.95, 1.0, a), 0.0);
}

/// The column from the menu bar onto the line and everything in it.
fn column(pen: &mut Pen, f: &Frame, bufs: &mut Buffers, flow: &mut Paint) {
    let ph = &f.ph;
    let ((lx, top), bar) = (f.land, f.bar);
    let y0 = bar + (top - bar) * ph.lift;
    let y1 = bar + (top - 1.0 - bar) * ph.drop;
    // the layers stop short of the fronts; the fronts carry them on
    let cy1 = (y1 - TAPER * ph.lead).max(y0);
    let cy0 = (y0 + TAPER * ph.trail).min(cy1);
    let col = [[(lx, cy0), (lx, cy1)]];
    let breathe = (1.0 + 0.1 * noise1(ph.run * 3.0, 1) + 1.2 * ph.surge) * ph.shrink;
    let k = ph.out * flicker(f.t) * (1.0 + 0.5 * ph.surge);
    // wide, soft, mid, halo; then the core
    pen.segs(&col, 90.0 * breathe, c4(0.25, 0.3, 1.0, 0.06 * k), 40.0);
    pen.segs(&col, 34.0 * breathe, c4(0.3, 0.45, 1.0, 0.14 * k), 14.0);
    let mid = 18.0 * std::f32::consts::SQRT_2 * breathe;
    pen.segs(&col, mid, c4(0.35, 0.6, 1.0, 0.2 * k), 6.0);
    pen.segs(&col, 6.0 * breathe, c4(0.6, 0.8, 1.0, 0.16 * k), 2.5);
    let w = CORE * breathe;
    core(pen, lx, (cy0, cy1), w * (1.0 + 0.5 * ph.surge), k);
    // the fronts stay between the menu bar and the line
    pen.c.save();
    pen.c
        .clip_rect(SkRect::new(-1e4, bar, 1e4, top - 1.0), None, true);
    front(pen, f, bufs, (lx, y1), 1.0, (w, k), ph.lead);
    front(pen, f, bufs, (lx, y0), -1.0, (w, k), ph.trail * 0.6);
    pen.c.restore();
    let [a, b, cc, d] = &mut bufs.dots;
    motes::flow(pen, f, (cy0, cy1), w, k, flow);
    let fil = (cy0, (cy1 + 25.0 * ph.lead).min(y1));
    motes::filaments(pen, f, fil, k, &mut bufs.lines);
    motes::grain(pen, f, (y0, y1), k, [&mut *a, &mut *b, &mut *cc]);
    let seen = ph.out * ph.drop;
    motes::streaks(pen, f, (y0, y1), seen, &mut bufs.segs);
    motes::dust(pen, f, (y0, top), seen, [&mut *a, &mut *b]);
    motes::embers(pen, f, seen, [a, d]);
}

/// The core: a soft luminous band, brightest in the middle, falling off
/// smoothly, with a faint thin spine rather than a hard line.
fn core(pen: &mut Pen, x: f32, (y0, y1): Pt, w: f32, k: f32) {
    let soft = |a: f32| c4(0.82, 0.9, 1.0, a);
    let colors = [
        soft(0.0),
        soft(0.2 * k),
        c4(0.92, 0.96, 1.0, 0.42 * k),
        soft(0.2 * k),
        soft(0.0),
    ];
    let stops = [0.0, 0.3, 0.5, 0.7, 1.0];
    let g = Gradient::new(
        Colors::new(&colors, Some(&stops), TileMode::Clamp, None),
        Interpolation::default(),
    );
    let mut p = Paint::default();
    p.set_anti_alias(true).set_blend_mode(BlendMode::Plus);
    p.set_shader(shaders::linear_gradient(
        ((x - w, 0.0), (x + w, 0.0)),
        &g,
        None,
    ));
    p.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, 2.0, None));
    pen.c.draw_rect(SkRect::new(x - w, y0, x + w, y1), &p);
    pen.segs(&[[(x, y0), (x, y1)]], 1.0, c4(1.0, 1.0, 1.0, 0.18 * k), 0.6);
}

/// A ragged, particle-made front at `edge`, facing `dir` (1 down: the
/// dropping tip; -1 up: the top as it draws down), `amt` of it, round a
/// core `w` wide at light `k`: the core tapers into a wandering point,
/// grain races ahead at different depths, hairline streaks with ragged
/// ends, a few fast sparks lead with trails.
fn front(
    pen: &mut Pen,
    f: &Frame,
    bufs: &mut Buffers,
    (x, edge): Pt,
    dir: f32,
    (w, k): Pt,
    amt: f32,
) {
    if amt <= 0.0 {
        return;
    }
    let t = f.t;
    let flick = (t * 15.0) as u32; // new raggedness 15 times a second
    // never behind the column's own ends: the menu bar above, the line below
    let base = if dir > 0.0 {
        (edge - TAPER).max(f.bar + 2.0)
    } else {
        (edge + TAPER).min(f.land.1 - 2.0)
    };
    for (i, (hw, reach, a)) in [(0.75, 18.0, 0.32), (0.3, 34.0, 0.4)]
        .into_iter()
        .enumerate()
    {
        let n = i as f32;
        let px = x + w * 0.25 * noise1(t * 6.0 + n * 5.0, 700 + i as u32);
        let py = edge + dir * reach * (0.7 + 0.5 * noise1(t * 9.0 + n, 710 + i as u32));
        let tip = |b: &mut PathBuilder| {
            b.move_to((x - w * hw, base));
            b.quad_to((x - w * hw * 0.5, edge), (px, py));
            b.quad_to((x + w * hw * 0.5, edge), (x + w * hw, base));
            b.close();
        };
        pen.fill(tip, c4(0.85, 0.92, 1.0, a * k * amt), 5.0 - 3.0 * n);
    }
    let [dim, bright, ..] = &mut bufs.dots;
    dim.clear();
    bright.clear();
    for j in 0..90u32 {
        let id = j * 7919 + flick / 2;
        let d = h2(id, 1).powi(2) * 110.0 - 35.0;
        let out = 1.0 - d.max(0.0) / 110.0;
        let px = x + (h2(id, 2) + h2(id, 3) - 1.0) * w * 0.9 * out.powf(1.5);
        let r = (0.25 + 0.8 * h2(id, 4).powi(2)) * out * amt; // it thins out by shrinking
        let into = if h2(id, 5) < 0.3 {
            &mut *bright
        } else {
            &mut *dim
        };
        into.push(((px, edge + dir * d), r));
    }
    pen.dots(dim, c4(0.8, 0.88, 1.0, 0.7 * k), 0.0);
    pen.dots(bright, c4(1.0, 1.0, 1.0, 0.95 * k), 0.0);
    pen.dots(bright, c4(0.5, 0.65, 1.0, 0.35 * k), 1.2);
    let [lines, sparks, ..] = &mut bufs.segs;
    lines.clear();
    for j in 0..12u32 {
        let id = j * 104_729 + flick;
        let px = x + (h2(id, 1) - 0.5) * 1.4 * w;
        let reach = 10.0 + 55.0 * h2(id, 2) * (1.0 - (px - x).abs() / w).max(0.2);
        lines.push([(px, edge - dir * 25.0), (px, edge + dir * reach)]);
    }
    pen.segs(lines, 0.5, c4(0.8, 0.9, 1.0, 0.55 * k * amt), 0.0);
    sparks.clear();
    dim.clear();
    for j in 0..5u32 {
        let id = j * 31 + flick / 3;
        let d = 30.0 + 60.0 * h2(id, 1);
        let px = x + (h2(id, 2) - 0.5) * 0.6 * w;
        let head = (px, edge + dir * d);
        sparks.push([(px, edge + dir * (d - 20.0 - 25.0 * h2(id, 3))), head]);
        dim.push((head, 0.9));
    }
    pen.segs(sparks, 0.7, c4(0.9, 0.95, 1.0, 0.8 * k * amt), 0.0);
    pen.segs(sparks, 2.5, c4(0.4, 0.6, 1.0, 0.3 * k * amt), 1.2);
    pen.dots(dim, c4(1.0, 1.0, 1.0, k * amt), 0.0);
}
