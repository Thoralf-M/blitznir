//! The beam sound's building blocks: a state-variable filter, a small
//! Freeverb, noise hashed from the sample index (so every render of the
//! same stretch of the stream is the same, whatever it starts from), and
//! the working layers.

use super::RATE;
use crate::beam::{noise1, smooth};
use std::f32::consts::{SQRT_2, TAU};

pub const SR: f32 = RATE as f32;

/// A TPT state-variable filter: low, band and high at once.
#[derive(Default, Clone, Copy)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    pub fn run(&mut self, x: f32, fc: f32, q: f32) -> (f32, f32, f32) {
        let g = (std::f32::consts::PI * fc.clamp(10.0, SR * 0.45) / SR).tan();
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + g * a1 * v3;
        let v2 = self.ic2 + g * v1;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, v1, x - k * v1 - v2)
    }
}

/// White noise, -1..1, for sample `i` of noise stream `stream` of `seed`
/// (splitmix64).
pub fn white(seed: u64, stream: u64, i: usize) -> f32 {
    let mut z = seed ^ stream.wrapping_mul(0xD1B5_4A32_D192_ED03) ^ (i as u64);
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

/// The stream's loudness in mono: a constant-power pan keeps a voice's
/// power (1), a voice on both channels at once doubles it (a stereo mix's
/// `s[c] += x`), and so do the reverb's two decorrelated sides.
pub const BOTH: f32 = SQRT_2;

/// A small Freeverb: eight combs and four allpasses.
pub struct Reverb {
    combs: Vec<(Vec<f32>, usize, f32)>,
    aps: Vec<(Vec<f32>, usize)>,
}

const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const APS: [usize; 4] = [556, 441, 341, 225];
const ROOM: f32 = 0.84;
const DAMP: f32 = 0.3;
const WET: f32 = 0.9;
/// Freeverb's input gain: its combs sum eight feedback lines.
const DRIVE: f32 = 0.015;

impl Reverb {
    pub fn new() -> Reverb {
        // Freeverb's delays are in samples at 44.1 kHz
        let len = |d: usize| ((d as f32 * SR / 44_100.0) as usize).max(1);
        Reverb {
            combs: COMBS.iter().map(|d| (vec![0.0; len(*d)], 0, 0.0)).collect(),
            aps: APS.iter().map(|d| (vec![0.0; len(*d)], 0)).collect(),
        }
    }

    /// `x` with its reverb. The room hears the sum of its two
    /// channels: twice a voice in the middle (`x`'s power is both
    /// channels'), but only once the part of `x` whose sides are apart.
    pub fn run(&mut self, x: f32, apart: f32) -> f32 {
        let input = ((x - apart) * BOTH + apart) * DRIVE;
        let mut y = 0.0;
        for (line, pos, store) in &mut self.combs {
            let o = line[*pos];
            *store = o * (1.0 - DAMP) + *store * DAMP;
            line[*pos] = input + *store * ROOM;
            *pos = (*pos + 1) % line.len();
            y += o;
        }
        for (line, pos) in &mut self.aps {
            let b = line[*pos];
            line[*pos] = y + b * 0.5;
            y = b - y;
            *pos = (*pos + 1) % line.len();
        }
        x + y * WET * BOTH
    }
}

/// In over `rise` from the start, out over `fall` from `fin`.
pub fn working(t: f32, fin: f32, rise: f32, fall: f32) -> f32 {
    smooth(t / rise) * (1.0 - smooth((t - fin) / fall))
}

/// ∫₀ˣ noise1(u, seed) du, exactly: a full cell of value noise integrates
/// to the mean of its ends, a part of one to smoothstep's integral.
fn noise_integral(x: f32, seed: u32) -> f64 {
    let cells = x.floor().max(0.0) as u32;
    let v = |i: u32| f64::from(crate::beam::h2(i, seed));
    let mut sum: f64 = (0..cells).map(|i| v(i) + v(i + 1) - 1.0).sum();
    let f = f64::from(x) - f64::from(cells);
    let (a, b) = (v(cells), v(cells + 1));
    sum += 2.0 * (a * f + (b - a) * (f.powi(3) - f.powi(4) / 2.0)) - f;
    sum
}

/// The low bed: three soft tones drifting in pitch, each on its own slow
/// noise, swelling, under a moving lowpass, and a rumble under them.
pub struct Bed {
    phase: [f64; 3],
    /// This job's transposition of [`LAYERS`]: no two hum alike.
    pitch: f32,
    lp: Svf,
    rumble: Svf,
}

const LAYERS: [(f32, f32); 3] = [(55.0, 1.0), (41.2, 0.45), (82.4, 0.4)];
const DRIFT: f32 = 0.015;
/// The hum and rumble's level: 0.07 less 3 dB, as the low end at 0.07 was
/// too loud through a whole job.
const BED_GAIN: f32 = 0.07 * 0.708;

fn drift_rate(n: usize) -> f32 {
    0.25 + 0.1 * n as f32
}

impl Bed {
    /// The bed as it is at stream time `t0`: its phases are the integral of
    /// its drifting pitch from 0, so a render from anywhere continues it.
    /// Its drifts and swells are fixed, not the seed's: they are slow
    /// enough that another seed is another loudness, up to 3.5 dB apart.
    pub fn new(t0: f32, pitch: f32) -> Bed {
        let phase = std::array::from_fn(|n| {
            let (f0, r) = (f64::from(LAYERS[n].0 * pitch), drift_rate(n));
            let drift = noise_integral(t0 * r, 510 + n as u32) / f64::from(r);
            (f0 * (f64::from(t0) + f64::from(DRIFT) * drift)).fract()
        });
        Bed {
            phase,
            pitch,
            lp: Svf::default(),
            rumble: Svf::default(),
        }
    }

    pub fn next(&mut self, t: f32, noise: f32, env: f32) -> f32 {
        let mut m = 0.0;
        for (n, (f0, l)) in LAYERS.iter().enumerate() {
            let p = self.phase[n] as f32 * TAU;
            let v = p.sin() + 0.35 * (2.0 * p).sin() + 0.15 * (3.0 * p).sin();
            let swell = 0.75 + 0.25 * noise1(t * (0.3 + 0.13 * n as f32), 520 + n as u32);
            m += v * l * swell;
            let drift = DRIFT * noise1(t * drift_rate(n), 510 + n as u32);
            let f = f0 * self.pitch * (1.0 + drift);
            self.phase[n] = (self.phase[n] + f64::from(f / SR)).fract();
        }
        let fc = 300.0 + 120.0 * noise1(t * 0.4, 530);
        let low = self.lp.run(m, fc, 0.7).0;
        let r = self.rumble.run(noise, 90.0, 0.6).0 * 1.6 * (0.6 + 0.4 * noise1(t * 0.7, 540));
        (low + r * BOTH) * env * BED_GAIN
    }
}
