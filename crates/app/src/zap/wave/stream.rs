//! One frame of the stream: a hairline wave with specks
//! flowing toward a soft glowing tip, a faint shimmer band along it. The
//! head tapers over [`TAIL`] points behind the tip: its brightness, width,
//! grain and the wave's wobble all follow one ease, so there is no seam.
//! A pure function of time, drawn with the [`Pen`]; and the chat's trail of
//! particles over the text just written.

use super::Pt;
use super::pen::Pen;
use super::sprite::Rgb;
use blitznir_core::beam::{ease, h2, noise1, smooth, walk};
use blitznir_core::wave::{Heads, TAIL, TIP_SLOW, TRAIL_SECS};
use std::f32::consts::TAU;

const DIM: Rgb = [0.8, 0.88, 1.0];
const WHITE: Rgb = [1.0, 1.0, 1.0];
const HALO: Rgb = [0.5, 0.65, 1.0];
const MID: Rgb = [0.95, 0.97, 1.0];
const SPIKE: Rgb = [0.8, 0.9, 1.0];

/// The particles' buffers, kept across frames.
#[derive(Default)]
pub struct Bufs {
    dots: [Vec<(Pt, f32)>; 2],
    pts: Vec<Pt>,
    segs: Vec<[Pt; 2]>,
}

/// One stream: seconds since it began, its span (the tip at `xb`, pointing
/// right), its line, its strength, and the gain of the tip's core light.
#[derive(Debug, Clone, Copy)]
pub struct Stream {
    pub t: f32,
    pub xa: f32,
    pub xb: f32,
    pub y: f32,
    pub k: f32,
    pub core: f32,
}

/// The shape every part shares: the taper, the wave, the breathing.
struct Shape {
    t: f32,
    xa: f32,
    xb: f32,
    xs: f32,
    sl: f32,
    y: f32,
    k: f32,
    /// The light's breathing (two slow sines) and the head's brightness.
    breath: f32,
    br: f32,
}

impl Shape {
    fn new(s: &Stream) -> Shape {
        let xa = s.xa.min(s.xb - 24.0);
        let sl = TAIL.min(s.xb - xa);
        let t = s.t;
        Shape {
            t,
            xa,
            xb: s.xb,
            xs: s.xb - sl,
            sl,
            y: s.y,
            k: s.k,
            breath: 1.0 + 0.1 * (t * 1.7).sin() + 0.06 * noise1(t * 0.9, 7),
            br: 1.0 + 0.12 * noise1(t * 3.0 * TIP_SLOW, 1),
        }
    }

    /// 0 along the tail, rising to 1 at the tip.
    fn e(&self, x: f32) -> f32 {
        ease((x - self.xs) / self.sl)
    }

    /// The wave flattens into the head's straight core along the taper.
    fn flat(&self, x: f32) -> f32 {
        1.0 - 0.85 * self.e(x)
    }

    fn wave(&self, x: f32) -> f32 {
        let t = self.t;
        (1.3 * noise1(x * 0.025 - t * 0.6, 11) + 0.5 * noise1(x * 0.07 - t * 1.4, 12))
            * self.flat(x)
    }

    /// The head's line: the wave, and a slow undulation through the head.
    fn mid(&self, x: f32) -> f32 {
        self.y + self.wave(x) + self.e(x) * 0.8 * (x * 0.09 - self.t * 1.3).sin()
    }
}

/// The stream `s`.
pub fn stream(pen: &mut Pen, s: &Stream, b: &mut Bufs) {
    if s.k <= 0.002 {
        return;
    }
    let sh = Shape::new(s);
    line(pen, &sh, b);
    specks(pen, &sh, b);
    head(pen, &sh, s.core);
    grain(pen, &sh, b);
    tip(pen, &sh, s.core, b);
}

/// The faint shimmer with its slow sweep of light, and the hairline: its
/// glow, and the line itself fading out before the tip (no crisp line runs into it).
fn line(pen: &mut Pen, sh: &Shape, b: &mut Bufs) {
    let (t, xa, xb, y, k) = (sh.t, sh.xa, sh.xb, sh.y, sh.k);
    let l = xb - xa;
    pen.band((xa, xb + 20.0), y, 6.0, 0.045 * k, 40.0);
    let sx = xa - 60.0 + (l + 120.0) * ease((t / 3.8).fract());
    let vis = smooth((sx - xa + 30.0) / 40.0) * smooth((xb + 20.0 - sx) / 40.0);
    let sp = pen.sprites;
    pen.img(&sp.flow, (sx - 55.0, y - 3.0, 110.0, 6.0), 0.1 * k * vis);
    b.pts.clear();
    let mut x = xa;
    while x < xb {
        b.pts.push((x, y + sh.wave(x)));
        x += 3.0;
    }
    b.pts.push((xb, y + sh.wave(xb)));
    pen.stroke_h(&b.pts, 2.4, HALO, 0.035 * k, (xa, xb + 30.0), 30.0);
    pen.stroke_h(&b.pts, 0.5, DIM, 0.2 * k, (xa, xb), 45.0);
}

/// Specks flowing along the wave toward the tip, brighter near it.
fn specks(pen: &mut Pen, sh: &Shape, b: &mut Bufs) {
    const LOOP: f32 = 640.0;
    let [dim, bright] = &mut b.dots;
    dim.clear();
    bright.clear();
    let t = sh.t;
    for j in 0..64u32 {
        let v = 8.0 + 22.0 * h2(j, 1);
        let x = sh.xa + (h2(j, 2) * LOOP + t * v) % LOOP;
        if x > sh.xb - 2.0 {
            continue;
        }
        let fade = smooth((x - sh.xa) / 24.0);
        let tw = 0.55 + 0.45 * (t * (1.5 + 3.0 * h2(j, 6)) + h2(j, 7) * TAU).sin();
        let flat = sh.flat(x);
        let (wx, wy) = walk(j, t, 1.1 * flat + 0.2);
        let r = (0.3 + 0.55 * h2(j, 3).powi(2)) * fade * (0.6 + 0.4 * tw);
        let into = if h2(j, 5) < 0.25 + 0.4 * sh.e(x) {
            &mut *bright
        } else {
            &mut *dim
        };
        let dy = wy * 0.8 + (h2(j, 4) - 0.5) * 1.6 * flat;
        into.push(((x + wx, sh.y + sh.wave(x) + dy), r));
    }
    pen.dots(dim, DIM, 0.55 * sh.k);
    pen.dots(bright, WHITE, 0.85 * sh.k);
}

/// The head's soft glow over the tail, and its core: a soft elongated
/// highlight riding the undulation, no stroke.
fn head(pen: &mut Pen, sh: &Shape, gain: f32) {
    let (t, xb, k, br) = (sh.t, sh.xb, sh.k, sh.br);
    let gh = 6.0 * br * (1.0 + 0.06 * (t * 1.7).sin());
    let glow = (sh.xs, sh.mid(xb - 20.0) - gh, sh.sl + 4.0, gh * 2.0);
    let sp = pen.sprites;
    pen.img(&sp.head, glow, 0.4 * k * sh.breath);
    let cl = 70.0f32.min(sh.sl);
    let ch = 2.6 * br * (1.0 + 0.08 * (t * 1.3).sin());
    let core = (xb - cl, sh.mid(xb - cl * 0.3) - ch, cl + 2.0, ch * 2.0);
    pen.img(&sp.core, core, 0.55 * k * sh.breath * gain);
}

/// Grain streaming to the tip: dense there, thinning out toward the tail.
fn grain(pen: &mut Pen, sh: &Shape, b: &mut Bufs) {
    const RATE: f32 = 110.0;
    let [dim, bright] = &mut b.dots;
    dim.clear();
    bright.clear();
    let (t, sl) = (sh.t, sh.sl);
    for n in ((t - 2.5) * RATE).floor().max(0.0) as u32..=(t * RATE).floor() as u32 {
        let age = t - n as f32 / RATE;
        let life = (0.4 + 0.9 * h2(n, 1)) / TIP_SLOW;
        if !(0.0..=life).contains(&age) {
            continue;
        }
        let d = age * TIP_SLOW * (40.0 + 110.0 * h2(n, 2).powf(1.5)) + h2(n, 6) * sl * 0.6;
        let x = sh.xs + d;
        let u = sh.e(x);
        if d > sl || h2(n, 7) > 0.1 + 0.9 * u {
            continue;
        }
        let (wx, wy) = walk(n, age * TIP_SLOW, 0.8);
        let fade = smooth(age / 0.08)
            * smooth((life - age) / 0.15)
            * smooth((sl - d) / 8.0)
            * (0.3 + 0.7 * u);
        let into = if h2(n, 5) < 0.6 {
            &mut *dim
        } else {
            &mut *bright
        };
        let y = sh.mid(x) + (h2(n, 3) - 0.5) * (1.0 + 2.5 * u) * sh.br + wy * 0.5;
        into.push(((x + wx, y), (0.2 + 0.5 * h2(n, 4).powi(2)) * fade));
    }
    pen.dots(dim, MID, 0.55 * sh.k);
    pen.dots(bright, WHITE, 0.95 * sh.k);
    let sp = pen.sprites;
    pen.imgs(&sp.dot, bright, 7.0, 0.3 * sh.k);
}

/// The tip: one soft pointed lens with a slightly ragged end, 4 faint
/// short spikes in a narrow cone and 4 specks just ahead, on slow noise.
fn tip(pen: &mut Pen, sh: &Shape, gain: f32, b: &mut Bufs) {
    let (t, xb, k, br, breath) = (sh.t * TIP_SLOW, sh.xb, sh.k, sh.br, sh.breath);
    let yt = sh.mid(xb);
    let sp = pen.sprites;
    pen.img(
        &sp.tip,
        (xb - 30.0, yt - 4.0 * br, 38.0, 8.0 * br),
        0.5 * k * breath,
    );
    b.segs.clear();
    for j in 0..4u32 {
        let n = j as f32;
        let reach = (4.0 + 7.0 * h2(j, 1)) * (0.6 + 0.4 * noise1(t * 2.2 + n * 1.7, 900 + j));
        let y1 = yt + (h2(j, 2) - 0.5) * 0.9 + 0.25 * noise1(t * 1.8 + n * 2.3, 920 + j);
        b.segs
            .push([(xb - 4.0, yt + (h2(j, 2) - 0.5) * 0.4), (xb + reach, y1)]);
    }
    pen.segs(&b.segs, 0.3, SPIKE, 0.22 * k * breath);
    let spray = &mut b.dots[0];
    spray.clear();
    for j in 0..4u32 {
        let n = j as f32;
        let d = 1.0 + 6.0 * h2(j, 11) + 1.5 * noise1(t * 1.5 + n, 950 + j);
        let y = yt + (h2(j, 12) - 0.5) * 0.8 + 0.3 * noise1(t * 1.2 + n * 3.0, 970 + j);
        spray.push(((xb + d, y), (0.25 + 0.3 * h2(j, 14)) * (1.0 - d / 10.0)));
    }
    pen.dots(spray, WHITE, 0.45 * k * breath);
    pen.img(
        &sp.bright,
        (xb - 14.0, yt - 2.0, 20.0, 4.0),
        0.16 * k * breath * gain,
    );
}

/// The chat's trail: particles born where the text ended over the last
/// [`TRAIL_SECS`], each fading as it ages, `t` seconds in, `k` strong.
pub fn trail(pen: &mut Pen, heads: &Heads, t: f32, k: f32, b: &mut Bufs) {
    const RATE: f32 = 90.0;
    if k <= 0.002 {
        return;
    }
    let [dim, bright] = &mut b.dots;
    dim.clear();
    bright.clear();
    for n in ((t - TRAIL_SECS) * RATE).floor().max(0.0) as u32..=(t * RATE).floor() as u32 {
        let born = n as f32 / RATE;
        let a = t - born;
        let Some((hx, hy)) = heads.at(born).filter(|_| (0.0..=TRAIL_SECS).contains(&a)) else {
            continue;
        };
        let f = (1.0 - a / TRAIL_SECS).powf(1.3);
        let (wx, wy) = walk(n, a * TIP_SLOW, 1.2);
        let into = if h2(n, 5) < 0.25 {
            &mut *bright
        } else {
            &mut *dim
        };
        let p = (
            hx - h2(n, 1) * 8.0 - a * 6.0 + wx,
            hy + (h2(n, 2) - 0.5) * 3.0 + wy * 0.6,
        );
        into.push((p, (0.25 + 0.45 * h2(n, 3).powi(2)) * f));
    }
    pen.dots(dim, DIM, 0.45 * k);
    pen.dots(bright, WHITE, 0.75 * k);
}
