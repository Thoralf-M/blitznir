//! The beam's sound, synthesised, no audio shipped: while it works, a low
//! drifting hum, a soft hiss and sparse crackle in a wide room, after a
//! short quiet opening whoosh; when the text lands, the crackle thins out
//! over faint air ("dust"), as full as the job was long. Mono; the numbers
//! were tuned by ear.
//!
//! A [`Synth`] makes it one sample after another, and the app plays that as
//! one stream from the start to the landing's end: the landing comes in at
//! the very sample it is asked for, with no gap and no file.

mod dsp;
#[cfg(test)]
mod tests;

use crate::beam::{noise1, smooth};
use dsp::{BOTH, Bed, Reverb, SR, Svf, white, working};
use std::f32::consts::PI;

/// The rate the noise layers and clicks were tuned at: they sound the same only at it.
pub const RATE: u32 = 48_000;
/// The landing's length: the dust has thinned out ([`DUST_SECS`]) and the
/// room's tail died away by then.
pub const LANDING_SECS: f32 = 2.0;
/// The mix's gain: a 3 s job plays at the thunder's loudness, -25.5 LUFS
/// from thunder-synth (measured with ffmpeg's ebur128).
const GAIN_DB: f32 = 0.5;
/// A job starts 0.5 s into the clock the slow drifts, swells and tremolos
/// run on: they were tuned by ear on that phase.
const PHASE_T0: f32 = 0.5;
/// The working sound fades this long after the landing starts, so the
/// landing's air rises just ahead of the fade.
const LEAD_SECS: f32 = 0.07;
/// The landing's last stretch fades out: nothing ends cut.
const FADE_OUT_SECS: f32 = 0.5;

/// Every job's own hum: its slow drifts and swells start this far into
/// their pattern (up to a minute, some 20 of their slowest swings; more
/// costs f32 precision), its pitch this far off (+-3 %), both from the
/// seed. Jobs then differ by up to about 3 dB (measured over 5 seeds' first
/// 6 s: -25.5 to -28.5 LUFS).
fn voice(seed: u64) -> (f32, f32) {
    let u = |stream| (white(seed, stream, 0) + 1.0) / 2.0;
    (60.0 * u(9), 1.0 + 0.06 * (u(10) - 0.5))
}

/// How full the landing is after a job of `ran` seconds: 0.45 (-7 dB) for
/// the shortest, -6 dB at 1 s, 0.85 at 1.9 s, all of it from 2.6 s (by ear,
/// -6 dB for a 1 s job, 0.8 for 2 s, full for 3 s): the hum carries
/// a short job, whose opening and landing would otherwise run
/// into one whoosh.
pub fn landing_scale(ran: f32) -> f32 {
    let e = smooth((ran - 0.6) / 2.0);
    0.45 + 0.55 * e
}

fn secs(s: f32) -> usize {
    (s.max(0.0) * SR) as usize
}

/// The beam's sound as it plays, at [`RATE`]: the working stream from its
/// opening, and after [`Synth::land`] the landing, which the stream fades
/// out under; [`Synth::done`] once the landing has played.
pub struct Synth {
    seed: u64,
    /// The next sample's index from the start.
    i: usize,
    off: f32,
    /// When the working sound fades (never, until it lands).
    fin: f32,
    bed: Bed,
    hiss: Svf,
    whoosh: Svf,
    room: Reverb,
    clicks: Clicks,
    landing: Option<Landing>,
    gain: f32,
}

impl Synth {
    pub fn new(seed: u64) -> Synth {
        let (off, pitch) = voice(seed);
        Synth {
            seed,
            i: 0,
            off,
            fin: f32::INFINITY,
            bed: Bed::new(PHASE_T0 + off, pitch),
            hiss: Svf::default(),
            whoosh: Svf::default(),
            room: Reverb::new(),
            clicks: Clicks::new(seed),
            landing: None,
            gain: 10f32.powf(GAIN_DB / 20.0),
        }
    }

    /// The text landed after a job of `ran` seconds: the landing starts at
    /// the next sample (once; a second call changes nothing).
    pub fn land(&mut self, ran: f32) {
        if self.landing.is_some() {
            return;
        }
        let (t, scale) = (self.i as f32 / SR, landing_scale(ran));
        self.fin = t + LEAD_SECS;
        self.clicks.land(self.fin, scale);
        self.landing = Some(Landing::new(t, scale));
    }

    /// The landing has played out: nothing more to hear.
    pub fn done(&self) -> bool {
        let t = self.i as f32 / SR;
        self.landing
            .as_ref()
            .is_some_and(|l| t >= l.at + LANDING_SECS)
    }

    /// The next sample, -1..1.
    pub fn sample(&mut self) -> f32 {
        let (seed, i) = (self.seed, self.i);
        self.i += 1;
        let t = i as f32 / SR;
        let fin = self.fin;
        let env = working(t, fin, 0.6, 0.6);
        let clock = t + PHASE_T0 + self.off;
        let mut x = self.bed.next(clock, white(seed, 1, i), env);
        // the hiss wanders, never on a cycle: a sine here repeats audibly
        let band = 6500.0 + 1500.0 * noise1(clock * 0.35, 550);
        let air = self.hiss.run(white(seed, 2, i), band, 0.9);
        let wave = 0.8 + 0.2 * noise1(clock * 1.1, 560);
        x += air.1 * working(t, fin, 1.3, 0.6) * 0.03 * wave;
        if t < OPENING_SECS {
            x += opening(&mut self.whoosh, t, white(seed, 3, i));
        }
        x += self.clicks.at(i);
        let (mut apart, mut out) = (0.0, 1.0);
        if let Some(l) = self.landing.as_mut() {
            let (sides, middle) = l.next(t, seed, i);
            x += sides + middle;
            apart = sides;
            let left = l.at + LANDING_SECS - t;
            out = smooth(left / FADE_OUT_SECS);
        }
        limit(self.room.run(x, apart) * self.gain * out)
    }
}

/// The opening whoosh: short and quiet whatever the job's length, which is
/// not known at the start (gain 0.12 x 0.32 x 0.4 over 0.2 s, by ear).
const OPENING_SECS: f32 = 0.2;

fn opening(f: &mut Svf, t: f32, noise: f32) -> f32 {
    let u = t / OPENING_SECS;
    let fc = 500.0 * (1800.0f32 / 500.0).powf(smooth(u));
    let env = (u * PI).sin().max(0.0).powf(1.5);
    f.run(noise, fc, 0.8).1 * env * 0.12 * 0.32 * 0.4
}

/// Sparse crackle, 70 a second: one click through a narrow band at 8 kHz,
/// on a grid of slots hashed from the seed, so any render finds the same.
/// Past the landing it carries on, thinning out over [`DUST_SECS`]: the
/// landing's "dust".
struct Clicks {
    seed: u64,
    fin: f32,
    ring: Vec<f32>,
    /// The landing's [`landing_scale`]: the tail is as full as the job was long.
    scale: f32,
}

const CLICK_RATE: f32 = 140.0; // slots a second, half of them a click
const CLICK_SECS: f32 = 0.012;
/// The crackle's thinning, as long as the hum's and the air's fade: it
/// never goes on alone after them (alone at the end, it is heard as a fault).
const DUST_SECS: f32 = 0.8;

impl Clicks {
    fn new(seed: u64) -> Clicks {
        let mut f = Svf::default();
        let ring = (0..secs(CLICK_SECS))
            .map(|i| f.run(if i == 0 { 1.0 } else { 0.0 }, 8000.0, 2.5).1)
            .collect();
        Clicks {
            seed,
            fin: f32::INFINITY,
            ring,
            scale: 1.0,
        }
    }

    /// From `fin` on, the crackle thins out, `scale` full.
    fn land(&mut self, fin: f32, scale: f32) {
        (self.fin, self.scale) = (fin, scale);
    }

    fn at(&self, i: usize) -> f32 {
        let t = i as f32 / SR;
        let first = ((t - CLICK_SECS) * CLICK_RATE).floor().max(0.0) as usize;
        let mut v = 0.0;
        for slot in first..=(t * CLICK_RATE) as usize {
            let u = |k: u64| (white(self.seed, 40 + k, slot) + 1.0) / 2.0;
            let at = (slot as f32 + u(1)) / CLICK_RATE;
            if u(0) >= 0.5 || at < 0.3 || at >= self.fin + DUST_SECS {
                continue;
            }
            let g = if at > self.fin {
                // thinning out: fewer and softer
                let left = 1.0 - (at - self.fin) / DUST_SECS;
                if u(3) > left * left {
                    continue;
                }
                left.powf(1.5) * (0.6 + 0.4 * self.scale) * 1.6
            } else {
                working(at, self.fin, 0.5, 0.2)
            };
            let Some(k) = i.checked_sub(secs(at)) else {
                continue;
            };
            if let Some(r) = self.ring.get(k) {
                v += r * g * (0.3 + 0.7 * u(2)) * 0.22;
            }
        }
        v
    }
}

/// The landing's own part, under the crackle thinning out ([`Clicks`]):
/// faint air, a narrow band at 5.2 kHz rising in 0.1 s and dying over
/// 0.6 s, `scale` full, through a gentle lowpass.
struct Landing {
    at: f32,
    scale: f32,
    air: Svf,
    lp: Svf,
}

impl Landing {
    fn new(at: f32, scale: f32) -> Landing {
        Landing {
            at,
            scale,
            air: Svf::default(),
            lp: Svf::default(),
        }
    }

    /// (the part whose sides are apart, the part in the middle), as the
    /// room hears them: the air is all apart.
    fn next(&mut self, t: f32, seed: u64, i: usize) -> (f32, f32) {
        let a = t - self.at;
        if a < 0.0 {
            return (0.0, 0.0);
        }
        let env = if a < 0.1 {
            smooth(a / 0.1)
        } else {
            (-(a - 0.1) / 0.6).exp()
        };
        let air = self.air.run(white(seed, 10, i), 5200.0, 0.8).1 * env * 0.07 * BOTH;
        (self.lp.run(air * self.scale, 2200.0, 0.6).0, 0.0)
    }
}

/// A soft knee above 0.9: never a hard clip.
fn limit(v: f32) -> f32 {
    let a = v.abs();
    if a < 0.9 {
        v
    } else {
        v.signum() * (0.9 + 0.099 * ((a - 0.9) / 0.099).tanh())
    }
}
