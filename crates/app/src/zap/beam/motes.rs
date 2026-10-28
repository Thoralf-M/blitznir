//! Everything that moves in and round the column, each particle its own
//! speed, size, brightness, life and random walk (hashed from its number,
//! so a frame is a pure function of time): soft clumps of light and grain
//! flowing slowly down the core, filaments wobbling round it, hairline
//! streaks beside it, dust in the air, embers off the flare; and the flare
//! on the line's top edge with its spray of sparks.

use super::Pt;
use super::draw::{Frame, flicker};
use super::paint::{Pen, c4};
use blitznir_core::beam::{h2, noise1, smooth, walk};
use freya::engine::prelude::Paint;
use std::f32::consts::TAU;

/// Twice the base counts and turbulence: sparser read as flat, by eye.
const MORE: f32 = 2.0;
/// The wide core: grain spread and filament swing twice the base.
const WIDE: f32 = 2.0;

/// Soft clumps of light flowing slowly down inside the core: the current
/// falling into the flare.
pub fn flow(pen: &mut Pen, f: &Frame, (y0, y1): Pt, w: f32, k: f32, paint: &mut Paint) {
    let (run, x, bar) = (f.ph.run, f.land.0, f.bar);
    let span = (y1 - bar).max(1.0);
    for j in 0..12u32 {
        let v = 50.0 + 70.0 * h2(j, 1);
        let y = bar + (h2(j, 2) * span + run * v) % span;
        if y < y0 || y > y1 {
            continue;
        }
        let dx = w * 0.35 * noise1(y * 0.01 - run * 0.8 + j as f32 * 3.1, 600 + j);
        let a = (0.08 + 0.1 * h2(j, 3)) * k * smooth((y1 - y) / 60.0);
        let size = (w * (0.55 + 0.35 * h2(j, 4)), 22.0 + 40.0 * h2(j, 5));
        pen.blob(paint, (x + dx, y), size, a);
    }
}

/// Hairline filaments round the core, wobbling irregularly: noise in
/// height and time, no fixed period.
pub fn filaments(pen: &mut Pen, f: &Frame, (y0, y1): Pt, k: f32, lines: &mut Vec<Vec<Pt>>) {
    let n = (5.0 + 4.0 * MORE) as usize;
    let (t, x) = (f.t, f.land.0);
    lines.resize_with(n, Vec::new);
    for (i, pts) in lines.iter_mut().enumerate() {
        let id = i as u32;
        let amp = (6.0 + 13.0 * h2(id, 1)) * WIDE;
        let rate = 0.6 + 1.6 * h2(id, 3);
        let scale = 0.012 + 0.03 * h2(id, 2);
        pts.clear();
        pts.extend((0..=110).map(|s| {
            let y = y0 + (y1 - y0) * s as f32 / 110.0;
            let a = amp * (0.55 + 0.45 * noise1(y * 0.015 - t * 0.9 * rate, id + 40));
            let w = noise1(y * scale - t * rate * 1.7, id)
                + 0.45 * noise1(y * scale * 2.9 - t * rate * 3.4, id + 90);
            (x + a * w * 1.3, y)
        }));
    }
    pen.lines(lines, 1.6, c4(0.4, 0.6, 1.0, 0.1 * k), 1.0);
    pen.lines(lines, 0.35, c4(0.8, 0.88, 1.0, 0.3 * k), 0.0);
}

/// Grain streaming down inside the column: each grain its own speed, size,
/// brightness and life, wandering sideways in streams that meander and
/// weave; now and then a cluster runs down together.
pub fn grain(pen: &mut Pen, f: &Frame, (y0, y1): Pt, k: f32, b: [&mut Vec<(Pt, f32)>; 3]) {
    let (run, x, bar) = (f.ph.run, f.land.0, f.bar);
    let [dim, mid, bright] = b;
    dim.clear();
    mid.clear();
    bright.clear();
    let mut add = |id: u32, born: f32, life: f32, v: f32, spread: f32, top: f32| {
        let age = run - born;
        let y = top + age * v;
        if !(0.0..life).contains(&age) || y < y0 || y > y1 {
            return;
        }
        let (wx, _) = walk(id, age, spread);
        let st = id % 5;
        let s = st as f32;
        let weave = 9.0 * WIDE * noise1(y * 0.012 - run * 0.9 + s * 7.3, 300 + st)
            + 2.0 * noise1(y * 0.035 - run * 2.2, 400 + st);
        let fade = smooth(age / 0.08) * smooth((life - age) / 0.15);
        let r = (0.18 + 0.7 * h2(id, 4).powi(2)) * fade;
        let px = x + (h2(id, 3) - 0.5) * 16.0 * WIDE + wx + weave;
        let into = match h2(id, 5) {
            b if b < 0.5 => &mut *dim,
            b if b < 0.85 => &mut *mid,
            _ => &mut *bright,
        };
        into.push(((px, y), r));
    };
    let rate = 130.0 * MORE;
    for n in (((run - 1.2) * rate) as i32).max(0)..=(run * rate) as i32 {
        let n = n as u32;
        let top = bar + (y1 - bar) * h2(n, 6);
        let v = 90.0 + 330.0 * h2(n, 2).powf(1.5);
        add(
            n,
            n as f32 / rate,
            0.6 + 1.4 * h2(n, 1),
            v,
            2.5 + 2.5 * MORE,
            top,
        );
    }
    let every = 0.55 / MORE;
    let last = (run / every) as i32;
    for cl in (last - 2).max(0)..=last {
        let cl = cl as u32;
        if h2(cl, 70) < 0.35 {
            continue;
        }
        let born = cl as f32 * every + h2(cl, 71) * every;
        let v = 150.0 + 200.0 * h2(cl, 72);
        let top = bar + (y1 - bar) * 0.6 * h2(cl, 73);
        for j in 0..(18.0 * MORE) as u32 {
            let id = 100_000 + cl * 97 + j;
            let (at, life) = (born + h2(id, 8) * 0.06, 0.5 + 0.4 * h2(id, 9));
            let top = top - h2(id, 11) * 30.0;
            add(id, at, life, v * (0.9 + 0.2 * h2(id, 10)), 3.0 * MORE, top);
        }
    }
    pen.dots(dim, c4(0.75, 0.85, 1.0, 0.35 * k), 0.0);
    pen.dots(mid, c4(0.95, 0.97, 1.0, 0.6 * k), 0.0);
    pen.dots(bright, c4(1.0, 1.0, 1.0, 0.9 * k), 0.0);
    pen.dots(bright, c4(0.5, 0.65, 1.0, 0.35 * k), 1.0);
}

/// Dust in the air round the column: each speck its own life, size,
/// brightness and random walk, drifting slowly down, some sideways.
pub fn dust(pen: &mut Pen, f: &Frame, (y0, y1): Pt, k: f32, b: [&mut Vec<(Pt, f32)>; 2]) {
    let (t, x) = (f.t, f.land.0);
    let [dim, bright] = b;
    dim.clear();
    bright.clear();
    let rate = 200.0 * MORE;
    for n in (((t - 4.0) * rate) as i32).max(0)..=(t * rate) as i32 {
        let n = n as u32;
        let life = 1.2 + 2.8 * h2(n, 1);
        let age = t - n as f32 / rate;
        if !(0.0..life).contains(&age) {
            continue;
        }
        let g = (h2(n, 2) + h2(n, 3) - 1.0) * 2.0;
        let (bx, by) = (x + g * 80.0, y0 + (y1 - y0) * h2(n, 4));
        let (vx, vy) = ((h2(n, 5) - 0.5) * 16.0, 4.0 + 16.0 * h2(n, 6));
        let (wx, wy) = walk(n, age, 5.0 + 5.0 * MORE);
        let (px, py) = (bx + vx * age + wx, by + vy * age + wy);
        if py > y1 - 2.0 || py < f.bar + 3.0 {
            continue;
        }
        let tw = 0.5 + 0.5 * (age * (2.0 + 5.0 * h2(n, 7)) + h2(n, 8) * TAU).sin();
        let fade = smooth(age / 0.4) * smooth((life - age) / 0.6) * (0.35 + 0.65 * tw);
        let r = (0.22 + 0.7 * h2(n, 9).powi(3)) * fade;
        if r > 0.08 {
            let into = if h2(n, 10) < 0.2 {
                &mut *bright
            } else {
                &mut *dim
            };
            into.push(((px, py), r));
        }
    }
    pen.dots(dim, c4(0.75, 0.75, 1.0, 0.7 * k), 0.0);
    pen.dots(bright, c4(0.92, 0.9, 1.0, 0.9 * k), 0.0);
}

/// Embers off the flare: specks rising and drifting sideways from where the
/// column lands, each its own speed and life, a puff of them as it lands.
pub fn embers(pen: &mut Pen, f: &Frame, k: f32, b: [&mut Vec<(Pt, f32)>; 2]) {
    let run = f.ph.run - 0.6;
    if run < 0.0 || k <= 0.0 {
        return;
    }
    let ((x, y), a) = (f.land, f.ph.after);
    let [dim, bright] = b;
    dim.clear();
    bright.clear();
    let puff = if a > 0.0 { 1.0 + (-a * 3.0).exp() } else { 1.0 };
    let rate = 45.0 * MORE * puff;
    for n in (((run - 2.5) * rate) as i32).max(0)..=(run * rate) as i32 {
        let n = n as u32;
        let life = 0.7 + 1.8 * h2(n, 1);
        let age = run - n as f32 / rate;
        if !(0.0..life).contains(&age) {
            continue;
        }
        let side = (h2(n, 2) - 0.5) * 2.0;
        let (vx, vy) = (side * (20.0 + 70.0 * h2(n, 3)), -(12.0 + 55.0 * h2(n, 4)));
        let (wx, wy) = walk(n + 5000, age, 4.0 + 4.0 * MORE);
        let slow = 1.0 - (-age * 1.5).exp();
        let px = x + side * 12.0 + vx * slow / 1.5 + wx;
        let py = y - 2.0 + vy * age + wy.min(0.0);
        if py > y - 1.0 {
            continue;
        }
        let fade = smooth(age / 0.1) * (1.0 - age / life).powf(1.3);
        let r = (0.2 + 0.8 * h2(n, 5).powi(3)) * fade;
        let into = if h2(n, 6) < 0.25 {
            &mut *bright
        } else {
            &mut *dim
        };
        into.push(((px, py), r));
    }
    pen.dots(dim, c4(0.85, 0.75, 1.0, 0.75 * k), 0.0);
    pen.dots(bright, c4(1.0, 0.97, 1.0, k), 0.0);
    pen.dots(bright, c4(0.6, 0.45, 1.0, 0.35 * k), 1.2);
}

/// Hairline streaks sliding down beside the column, some brighter.
pub fn streaks(pen: &mut Pen, f: &Frame, (y0, y1): Pt, k: f32, segs: &mut [Vec<[Pt; 2]>; 3]) {
    let (run, x) = (f.ph.run, f.land.0);
    let [dim, bright, _] = segs;
    dim.clear();
    bright.clear();
    for j in 0..22u32 {
        let side = if j % 2 == 0 { -1.0 } else { 1.0 };
        let dx = side * (7.0 * WIDE + 48.0 * h2(j, 9).powf(1.3));
        let period = 0.5 + 0.8 * h2(j, 1);
        let last = ((run - h2(j, 2) * period) / period).floor() as i32;
        for n in (last - 3).max(0)..=last {
            let n = n as u32;
            let born = h2(j, 2) * period + n as f32 * period;
            let id = j * 97 + n;
            let head = y0 + (run - born) * (140.0 + 260.0 * h2(id, 3));
            let len = 20.0 + 130.0 * h2(id, 4);
            let (a, b) = ((head - len).max(y0), head.min(y1 - 4.0));
            if b > a {
                let into = if h2(id, 5) < 0.3 {
                    &mut *bright
                } else {
                    &mut *dim
                };
                into.push([(x + dx, a), (x + dx, b)]);
            }
        }
    }
    pen.segs(dim, 0.4, c4(0.6, 0.78, 1.0, 0.5 * k), 0.0);
    pen.segs(bright, 0.7, c4(0.85, 0.9, 1.0, 0.75 * k), 0.0);
    pen.segs(bright, 2.4, c4(0.4, 0.55, 1.0, 0.2 * k), 1.2);
}

/// The flare on the line's top edge, `k` strong: layered blobs (the
/// [`super::Cache`]'s gradients), a hairline horizon, a spray of fine
/// sparks hopping off the surface; wider in the lift where it lands.
pub fn flare(
    pen: &mut Pen,
    f: &Frame,
    k: f32,
    blobs: &mut [Paint; 6],
    sparks: &mut Vec<(Pt, f32)>,
) {
    if k <= 0.0 {
        return;
    }
    let ((x, y), run, surge) = (f.land, f.ph.run, f.ph.surge);
    let flick = flicker(f.t + 3.0);
    let w = 1.0 + 2.2 * surge;
    let line = 320.0 + 420.0 * surge;
    let [horizon, haze, glow, bright, hot, bloom] = blobs;
    pen.blob(horizon, (x, y - 0.3), (line, 0.8 + 0.6 * surge), k);
    pen.blob(haze, (x, y), (line * 0.6, 3.0 + 2.0 * surge), k);
    pen.blob(glow, (x, y), (110.0 * w, 14.0 * w), k * flick);
    pen.blob(bright, (x, y), (30.0 * w, 5.0 * w), k);
    pen.blob(hot, (x, y), (8.0 * w, 2.0 * w), k);
    pen.blob(bloom, (x, y - 4.0), (45.0 * w, 30.0 * w), k);
    let rate = 90.0 * MORE;
    let life = 0.8;
    sparks.clear();
    for n in (((run - life) * rate) as i32).max(0)..=(run * rate) as i32 {
        let n = n as u32;
        let age = run - n as f32 / rate;
        let life = life * (0.35 + 0.65 * h2(n, 9));
        if !(0.0..life).contains(&age) {
            continue;
        }
        let vx = (h2(n, 1) - 0.5) * (80.0 + 260.0 * h2(n, 8)) * (1.0 + surge);
        let vy = -(40.0 + 120.0 * h2(n, 2));
        let (px, py) = (x + vx * age, y - 1.0 + vy * age + 380.0 * age * age);
        if py < y - 0.5 {
            sparks.push(((px, py), (0.25 + 0.4 * h2(n, 3)) * (1.0 - age / life)));
        }
    }
    pen.dots(sparks, c4(0.95, 0.9, 1.0, 0.9 * k), 0.0);
}
