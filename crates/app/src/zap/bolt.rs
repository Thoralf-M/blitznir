//! One strike's shape: a main path by
//! midpoint displacement from the hammer (or the top edge) to a new spot in
//! the aim every time, a few short side branches off it, and a faint chain
//! that runs out from where it lands along the selected line, both ways.

use blitznir_core::platform::Rect;
use std::{collections::hash_map::RandomState, hash::BuildHasher};

pub type Pt = (f32, f32);

/// The main path's roughness and depth: 2^7 segments, big wanders first,
/// fine crackle last.
const ROUGH: f32 = 0.16;
const LEVELS: u32 = 7;
/// The branches': rougher, shallower.
const BRANCH_ROUGH: f32 = 0.25;
const BRANCH_LEVELS: u32 = 4;
/// The chain's hops: about this long, rough, a few levels of crackle.
const HOP: f32 = 44.0;
const HOPS_MAX: usize = 6;
const CHAIN_ROUGH: f32 = 0.45;
const CHAIN_LEVELS: u32 = 3;
/// A branch's reach, in shares of the strike's length, and its turn off
/// the main path's heading (radians): short and close for every strike,
/// long and spread wide for the last.
const REACH: (f32, f32) = (0.06, 0.2);
const TURN: (f32, f32) = (0.35, 0.95);
const LAST_REACH: (f32, f32) = (0.18, 0.45);
const LAST_TURN: (f32, f32) = (0.4, 1.2);
/// The last strike's extra branches, and how much bigger it is.
const LAST_BRANCHES: usize = 6;
const LAST_SCALE: f32 = 1.6;
/// The last strike's twigs: one off each branch, this share of its length.
const TWIGS: usize = 1;
const TWIG_REACH: (f32, f32) = (0.35, 0.6);
const TWIG_LEVELS: u32 = 3;

pub struct Bolt {
    pub main: Vec<Pt>,
    pub side: Vec<Vec<Pt>>,
    /// The last strike's forks off its branches; none on the others.
    pub twigs: Vec<Vec<Pt>>,
    /// From the impact out along the line: the hops to the left, then to
    /// the right, each in the order they light up.
    pub chain: [Vec<Vec<Pt>>; 2],
    /// 1, or [`LAST_SCALE`] for the last strike: widths, glow, flare.
    pub k: f32,
}

/// xorshift64*, seeded from std's per-process random hasher keys: a strike
/// needs a few hundred numbers, none of them secret.
pub struct Rng(u64);

impl Rng {
    pub fn new() -> Rng {
        Rng(RandomState::new().hash_one(0u8) | 1)
    }

    /// Uniform in `lo..hi`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let bits = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40; // 24 bits
        lo + (hi - lo) * (bits as f32 / (1u32 << 24) as f32)
    }
}

/// A path from `a` to `b`: split every segment, push the midpoint sideways
/// by up to `rough` times its length, halve the push, repeat.
fn fork(a: Pt, b: Pt, rough: f32, levels: u32, rng: &mut Rng) -> Vec<Pt> {
    let mut pts = vec![a, b];
    let mut push = (b.0 - a.0).hypot(b.1 - a.1) * rough;
    for _ in 0..levels {
        let mut out = Vec::with_capacity(pts.len() * 2 - 1);
        out.push(pts[0]);
        for w in pts.windows(2) {
            let (p, q) = (w[0], w[1]);
            let (dx, dy) = (q.0 - p.0, q.1 - p.1);
            let len = dx.hypot(dy).max(1.0);
            let off = rng.range(-push, push);
            let mid = (
                (p.0 + q.0) / 2.0 - dy / len * off,
                (p.1 + q.1) / 2.0 + dx / len * off,
            );
            out.extend([mid, q]);
        }
        pts = out;
        push /= 2.0;
    }
    pts
}

/// 2 to 4 side branches (`extra` more) off the main path, turned away from
/// its heading by `turn`, `reach` of its length long.
fn branches(
    main: &[Pt],
    (a, b): (Pt, Pt),
    extra: usize,
    last: bool,
    rng: &mut Rng,
) -> Vec<Vec<Pt>> {
    let (reach, turn) = if last {
        (LAST_REACH, LAST_TURN)
    } else {
        (REACH, TURN)
    };
    let dist = (b.0 - a.0).hypot(b.1 - a.1);
    let heading = (b.1 - a.1).atan2(b.0 - a.0);
    let n = rng.range(2.0, 5.0) as usize + extra;
    let len = main.len() as f32;
    // sides alternate from a random first: the branches spread both ways
    let first = if rng.range(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 };
    (0..n)
        .map(|i| {
            let p = main[(rng.range(len * 0.1, len * 0.8) as usize).min(main.len() - 1)];
            let side = if i % 2 == 0 { first } else { -first };
            let turn = heading + side * rng.range(turn.0, turn.1);
            let reach = dist * rng.range(reach.0, reach.1);
            let end = (p.0 + turn.cos() * reach, p.1 + turn.sin() * reach);
            fork(p, end, BRANCH_ROUGH, BRANCH_LEVELS, rng)
        })
        .collect()
}

/// [`TWIGS`] twigs off each branch, from its middle, turned off the
/// branch's heading: the last strike forks and forks again.
fn twigs(side: &[Vec<Pt>], rng: &mut Rng) -> Vec<Vec<Pt>> {
    side.iter()
        .flat_map(|branch| std::iter::repeat_n(branch, TWIGS))
        .map(|branch| {
            let (a, b) = (branch[0], branch[branch.len() - 1]);
            let len = branch.len() as f32;
            let p = branch[(rng.range(len * 0.3, len * 0.7) as usize).min(branch.len() - 1)];
            let sign = if rng.range(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 };
            let turn = (b.1 - a.1).atan2(b.0 - a.0) + sign * rng.range(0.3, 0.9);
            let reach = (b.0 - a.0).hypot(b.1 - a.1) * rng.range(TWIG_REACH.0, TWIG_REACH.1);
            let end = (p.0 + turn.cos() * reach, p.1 + turn.sin() * reach);
            fork(p, end, BRANCH_ROUGH, TWIG_LEVELS, rng)
        })
        .collect()
}

/// Hops from `from` along the line at its height to `to_x`, [`HOP`] long
/// at most [`HOPS_MAX`] of them, each a rough little arc.
fn hops(from: Pt, to_x: f32, rng: &mut Rng) -> Vec<Vec<Pt>> {
    let n = (((to_x - from.0).abs() / HOP).ceil() as usize).min(HOPS_MAX);
    let step = (to_x - from.0) / n.max(1) as f32;
    (0..n)
        .map(|i| {
            let a = (from.0 + step * i as f32, from.1);
            let b = (from.0 + step * (i + 1) as f32, from.1);
            fork(a, b, CHAIN_ROUGH, CHAIN_LEVELS, rng)
        })
        .collect()
}

/// A strike into `aim` (the overlay's points) from `src`, the hammer's
/// face; None: from the top edge above the target.
pub fn strike(aim: Rect, src: Option<(f64, f64)>, last: bool, rng: &mut Rng) -> Bolt {
    let (x, y, w, h) = (aim.x as f32, aim.y as f32, aim.w as f32, aim.h as f32);
    let inset = (w / 4.0).min(6.0);
    let b = (
        rng.range(x + inset, x + w - inset),
        y + h * rng.range(0.3, 0.7),
    );
    let a = src.map_or_else(
        || (b.0 + rng.range(-80.0, 80.0), 0.0),
        |(x, y)| (x as f32, y as f32),
    );
    let main = fork(a, b, ROUGH, LEVELS, rng);
    let extra = if last { LAST_BRANCHES } else { 0 };
    let side = branches(&main, (a, b), extra, last, rng);
    let twigs = if last { twigs(&side, rng) } else { vec![] };
    let k = if last { LAST_SCALE } else { 1.0 };
    let chain = [hops(b, x, rng), hops(b, x + w, rng)];
    Bolt {
        main,
        side,
        twigs,
        chain,
        k,
    }
}

/// The strike's flicker: flash, dip, re-strike, die away,
/// as the opacity `t` of the way through it.
pub fn flicker(t: f32) -> f32 {
    const KEYS: [(f32, f32); 6] = [
        (0.0, 0.0),
        (0.04, 1.0),
        (0.13, 0.2),
        (0.2, 1.0),
        (0.42, 0.65),
        (1.0, 0.0),
    ];
    if !(0.0..1.0).contains(&t) {
        return 0.0;
    }
    let i = KEYS
        .iter()
        .position(|(at, _)| *at > t)
        .unwrap_or(KEYS.len() - 1);
    let ((t0, v0), (t1, v1)) = (KEYS[i - 1], KEYS[i]);
    v0 + (v1 - v0) * (t - t0) / (t1 - t0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    const AIM: Rect = Rect {
        x: 300.0,
        y: 200.0,
        w: 420.0,
        h: 17.0,
    };

    #[test]
    fn the_main_path_runs_from_the_hammer_to_a_spot_in_the_aim() {
        let mut rng = Rng::new();
        for last in [false, true] {
            let bolt = strike(AIM, Some((1200.0, 12.0)), last, &mut rng);
            assert_eq!(bolt.main.len(), (1 << LEVELS) + 1);
            assert_eq!(bolt.main[0], (1200.0, 12.0));
            let (x, y) = *bolt.main.last().unwrap();
            assert!(AIM.contains((f64::from(x), f64::from(y))), "{x}, {y}");
            let n = bolt.side.len() - if last { LAST_BRANCHES } else { 0 };
            assert!((2..=4).contains(&n), "{n} branches");
            assert!(
                bolt.side
                    .iter()
                    .all(|b| b.len() == (1 << BRANCH_LEVELS) + 1)
            );
            assert_eq!(bolt.k, if last { LAST_SCALE } else { 1.0 });
            let twigs = if last { bolt.side.len() * TWIGS } else { 0 };
            assert_eq!(bolt.twigs.len(), twigs, "twigs off each last branch");
        }
    }

    #[test]
    fn the_chain_runs_from_the_impact_to_both_ends_of_the_line() {
        let bolt = strike(AIM, Some((1200.0, 12.0)), false, &mut Rng::new());
        let (x, y) = *bolt.main.last().unwrap();
        for (side, end) in bolt
            .chain
            .iter()
            .zip([AIM.x as f32, (AIM.x + AIM.w) as f32])
        {
            assert!(side.len() <= HOPS_MAX);
            let Some(first) = side.first() else { continue };
            assert_eq!(first[0], (x, y), "starts where the bolt landed");
            let last = *side.last().unwrap().last().unwrap();
            assert!(
                (last.0 - end).abs() < 0.01 && (last.1 - y).abs() < 0.01,
                "{last:?}"
            );
            // a hop's crackle may tilt a little past the ends: at most the
            // second level's push, half a hop times its roughness
            let slack = HOP * CHAIN_ROUGH / 2.0;
            for hop in side {
                let lo = x.min(end) - slack;
                let hi = x.max(end) + slack;
                assert!(
                    hop.iter().all(|p| (lo..=hi).contains(&p.0)),
                    "stays on the line"
                );
            }
        }
    }

    #[test]
    fn without_a_hammer_it_comes_from_the_top_edge_above_the_target() {
        let bolt = strike(AIM, None, false, &mut Rng::new());
        let (x, y) = bolt.main[0];
        assert_eq!(y, 0.0);
        assert!((bolt.main.last().unwrap().0 - x).abs() <= 80.0);
    }

    #[test]
    fn branches_turn_away_from_the_heading_and_stay_short() {
        let mut rng = Rng::new();
        let (a, b) = ((0.0, 0.0), (0.0, 1000.0));
        let main = fork(a, b, ROUGH, LEVELS, &mut rng);
        for side in branches(&main, (a, b), 0, false, &mut rng) {
            let (p, q) = (side[0], *side.last().unwrap());
            let reach = (q.0 - p.0).hypot(q.1 - p.1);
            assert!((59.0..=201.0).contains(&reach), "{reach}");
            let turn = ((q.1 - p.1).atan2(q.0 - p.0) - PI / 2.0).abs();
            assert!((0.34..=0.96).contains(&turn), "{turn}");
        }
    }

    #[test]
    fn the_flicker_flashes_dips_restrikes_and_dies() {
        assert_eq!(flicker(0.0), 0.0);
        assert!((flicker(0.04) - 1.0).abs() < 1e-6);
        assert!((flicker(0.13) - 0.2).abs() < 1e-6);
        assert!((flicker(0.2) - 1.0).abs() < 1e-6);
        assert!((flicker(0.71) - 0.325).abs() < 1e-6);
        assert_eq!(flicker(1.0), 0.0);
        assert_eq!(flicker(-0.1), 0.0);
    }
}
