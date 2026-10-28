//! The beam's timeline and noise, shared by its drawing (the app's
//! zap/beam.rs) and its sound ([`crate::beam_sound`]). Every part of the
//! beam is a pure function of the seconds since it started and since it
//! ended, so it runs for as long as the job does, at any frame rate. The
//! numbers were tuned by eye.

/// The menu-bar thread reaches the landing point over this.
const REACH_SECS: f32 = 0.3;
/// The column drops from the menu bar after this, eased over [`DROP_SECS`].
const DROP_AT: f32 = 0.2;
const DROP_SECS: f32 = 0.4;
/// The drop's ragged front thins out over the drop's last 15 %.
const LEAD_FROM: f32 = 0.85;
/// The lift where it lands, modest: three times as much reads as a flash.
const SURGE: f32 = 0.33;
const SURGE_PEAK_SECS: f32 = 0.05;
const SURGE_DECAY: f32 = 5.0;
/// The collapse from the top: it narrows from 0.2 s, its top comes down
/// from 0.25 s over 0.4 s, and its light fades from 0.3 s over 0.85 s.
const SHRINK_AT: f32 = 0.2;
const SHRINK_SECS: f32 = 0.45;
const SHRINK_BY: f32 = 0.7;
const LIFT_AT: f32 = 0.25;
const LIFT_SECS: f32 = 0.4;
const FADE_AT: f32 = 0.3;
const FADE_SECS: f32 = 0.85;
/// The flare where it lands lights up as the column arrives.
const FLARE_AT: f32 = 0.52;
const FLARE_SECS: f32 = 0.14;
/// A cancel runs the collapse this much faster, without the lift: gone in
/// a third of a second, as the panel comes back.
const CANCEL_PACE: f32 = 3.5;

/// Seconds after the end until nothing is left, landed or cancelled.
pub const LAND_SECS: f32 = FADE_AT + FADE_SECS;
pub const CANCEL_SECS: f32 = LAND_SECS / CANCEL_PACE;

/// How the beam ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// The answer is in: the lift where it lands, then the collapse.
    Land,
    /// Esc or an error: a quick collapse, no lift.
    Cancel,
}

/// The beam's state at one moment, every value 0 to 1 unless it says.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Phase {
    /// Seconds since the start.
    pub run: f32,
    /// Seconds since the end, on the landing's clock (a cancel runs it
    /// faster); negative while it runs.
    pub after: f32,
    /// How far the menu-bar thread has run toward the landing point.
    pub reach: f32,
    /// The thread's light: it goes as the collapse starts.
    pub bar: f32,
    /// How far the column has dropped from the menu bar to the text.
    pub drop: f32,
    /// How far its top has come down in the collapse.
    pub lift: f32,
    /// Its width: 1, narrowing to 0.3 in the collapse.
    pub shrink: f32,
    /// The lift where it lands, 0 to [`SURGE`].
    pub surge: f32,
    /// Its whole light, fading out after the end.
    pub out: f32,
    /// The ragged front on the dropping tip, and on the collapsing top.
    pub lead: f32,
    pub trail: f32,
    /// The flare on the line's top edge.
    pub flare: f32,
}

impl Phase {
    /// `run` seconds after the start; `end`: seconds since it ended, and how.
    pub fn at(run: f32, end: Option<(f32, End)>) -> Phase {
        let (a, landed) = match end {
            Some((secs, End::Land)) => (secs, true),
            Some((secs, End::Cancel)) => (secs * CANCEL_PACE, false),
            None => (-1.0, false),
        };
        let ended = a > 0.0;
        let drop = ease((run - DROP_AT) / DROP_SECS);
        let lift = if ended {
            ease((a - LIFT_AT) / LIFT_SECS)
        } else {
            0.0
        };
        let surge = if ended && landed {
            let decay = (-(a - SURGE_PEAK_SECS).max(0.0) * SURGE_DECAY).exp();
            SURGE * decay * smooth(a / SURGE_PEAK_SECS)
        } else {
            0.0
        };
        Phase {
            run,
            after: a,
            reach: ease(run / REACH_SECS),
            bar: if ended {
                (1.0 - a / FADE_AT).max(0.0)
            } else {
                1.0
            },
            drop,
            lift,
            shrink: if ended {
                1.0 - SHRINK_BY * ease((a - SHRINK_AT) / SHRINK_SECS)
            } else {
                1.0
            },
            surge,
            out: if ended {
                1.0 - ease((a - FADE_AT) / FADE_SECS)
            } else {
                1.0
            },
            lead: if drop < 1.0 {
                1.0 - smooth((drop - LEAD_FROM) / (1.0 - LEAD_FROM))
            } else {
                0.0
            },
            trail: if lift > 0.0 && lift < 1.0 { 1.0 } else { 0.0 },
            flare: smooth((run - FLARE_AT) / FLARE_SECS),
        }
    }

    /// Nothing is left to draw.
    pub fn gone(&self) -> bool {
        self.after >= LAND_SECS
    }

    /// The menu-bar hammer's glow, 0 to 1: up at once, a gentle flicker,
    /// down over 0.6 s after the end.
    pub fn hammer(&self) -> f32 {
        if self.after > 0.0 {
            return (1.0 - self.after / 0.6).max(0.0);
        }
        smooth(self.run / 0.15) * (0.7 + 0.15 * noise1(self.run * 4.0, 3))
    }
}

/// Ease in and out (smootherstep): the drop and the collapse never slide
/// at an even speed.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    (t * t * t * (t * (t * 6.0 - 15.0) + 10.0)).min(1.0)
}

/// Smoothstep of `t`, clamped to 0..1.
pub fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// An integer hashed to 0..1: every particle's own speed, size and life.
fn h1(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Two integers hashed to 0..1.
pub fn h2(a: u32, b: u32) -> f32 {
    h1(a.wrapping_mul(0x632B_E5AB) ^ b.wrapping_add(0x1B87_3593))
}

/// 1D value noise, smooth, about -1..1.
pub fn noise1(x: f32, seed: u32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let a = h2(i as i32 as u32, seed);
    let b = h2((i as i32 + 1) as u32, seed);
    (a + (b - a) * smooth(f)) * 2.0 - 1.0
}

/// A random walk of its own for particle `id`, `amp` far either way:
/// smooth, irregular, never in step with another (its own seeds and rates).
pub fn walk(id: u32, age: f32, amp: f32) -> (f32, f32) {
    let (fx, fy) = (0.6 + 1.8 * h2(id, 91), 0.5 + 1.5 * h2(id, 92));
    let x = noise1(age * fx + h2(id, 93) * 50.0, id) + 0.5 * noise1(age * fx * 2.7, id + 7);
    let y = noise1(age * fy + h2(id, 94) * 50.0, id + 13) + 0.5 * noise1(age * fy * 3.1, id + 19);
    (x * amp, y * amp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn the_column_drops_eased_with_a_ragged_front_that_goes_as_it_lands() {
        let p = |run| Phase::at(run, None);
        assert_eq!(p(0.2).drop, 0.0);
        assert!(close(p(0.4).drop, 0.5));
        assert_eq!(p(0.6).drop, 1.0);
        // eased: slow off the menu bar, slow onto the text
        assert!(p(0.22).drop < 0.01 && p(0.58).drop > 0.99);
        assert_eq!(p(0.3).lead, 1.0);
        assert_eq!(p(0.7).lead, 0.0);
        assert!(close(p(0.3).reach, 1.0));
        assert_eq!((p(2.0).lift, p(2.0).out, p(2.0).surge), (0.0, 1.0, 0.0));
    }

    #[test]
    fn landing_lifts_a_little_then_collapses_from_the_top() {
        let p = |a| Phase::at(3.0, Some((a, End::Land)));
        assert!(close(p(SURGE_PEAK_SECS).surge, SURGE), "the peak");
        assert!(p(0.5).surge < 0.05);
        assert_eq!(p(LIFT_AT).lift, 0.0);
        assert!(close(p(LIFT_AT + LIFT_SECS / 2.0).lift, 0.5));
        assert_eq!(p(LIFT_AT + LIFT_SECS).lift, 1.0);
        assert_eq!(p(0.45).trail, 1.0, "ragged while it draws down");
        assert_eq!(p(0.8).trail, 0.0);
        assert!(close(p(1.0).shrink, 1.0 - SHRINK_BY));
        assert!(!p(1.1).gone() && p(LAND_SECS).gone());
        assert_eq!(p(LAND_SECS).out, 0.0);
    }

    #[test]
    fn a_cancel_collapses_fast_without_the_lift() {
        let p = |a| Phase::at(3.0, Some((a, End::Cancel)));
        const { assert!(CANCEL_SECS < 0.35) };
        assert!((0..40).all(|i| p(i as f32 * 0.01).surge == 0.0));
        assert!(p(CANCEL_SECS).gone() && !p(CANCEL_SECS * 0.9).gone());
    }

    #[test]
    fn the_hammer_glows_while_it_works_and_dies_down_after() {
        assert_eq!(Phase::at(0.0, None).hammer(), 0.0);
        let g = Phase::at(1.0, None).hammer();
        assert!((0.55..=0.85).contains(&g), "{g}");
        assert!(close(Phase::at(3.0, Some((0.3, End::Land))).hammer(), 0.5));
        assert_eq!(Phase::at(3.0, Some((0.6, End::Land))).hammer(), 0.0);
    }

    #[test]
    fn noise_is_smooth_and_in_range() {
        let mut last = noise1(0.0, 7);
        for i in 1..4000 {
            let v = noise1(i as f32 * 0.001, 7);
            assert!((-1.0..=1.0).contains(&v));
            assert!((v - last).abs() < 0.01, "no jumps");
            last = v;
        }
        assert!((0..1000).all(|n| (0.0..1.0).contains(&h2(n, 3))));
    }
}
