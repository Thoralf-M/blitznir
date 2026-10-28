//! The wave's timeline: a hairline stream of light that marks where text
//! arrives, drawn by the app's zap/wave in two places: under the caret line
//! of the app being dictated into while a dictation records ([`spot`]), and
//! through the chat answer's line while it is asked and streams in. Its tip
//! follows the text through [`Follow`], a damped spring stepped once a
//! frame; the rest is a pure function of time, as core's beam is. The
//! numbers were tuned by eye.

use crate::beam::{ease, smooth};
use crate::platform::Rect;
use std::collections::VecDeque;
use std::time::Duration;

/// The head's taper behind the tip, in points: its glow, grain and the
/// wave's flattening all follow one curve over it.
pub const TAIL: f32 = 140.0;
/// The head's grain, walk and flicker run slower than the stream's, so the
/// tip reads as calm.
pub const TIP_SLOW: f32 = 0.6;
/// In the chat, the tip runs this far ahead of the text head.
pub const LEAD: f32 = 80.0;
/// The tip's first-order lag, then its spring's stiffness across and down:
/// no overshoot (both kernels are positive, so the tip never passes the
/// text), about 0.45 s to settle, a burst of words one glide. Down is
/// stiffer: on a wrap the tip drops to the new line first.
const LAG: f32 = 0.06;
const WX: f32 = 12.0;
const WY: f32 = 22.0;
/// How far a moving target is trailed: the chat's target leads by the
/// head's speed times this, so the visible lead stays [`LEAD`].
pub const FOLLOW_LAG: f32 = LAG + 2.0 / WX;
/// Steps of at most this, however long a frame took.
const STEP: f32 = 1.0 / 120.0;
/// The head's speed is measured over this.
const SPEED_SECS: f32 = 0.4;
/// The chat's trail over fresh text, and its new line: the stream grows in
/// from the line's left edge while the last line's fades.
pub const TRAIL_SECS: f32 = 0.5;
pub const GROW_SECS: f32 = 0.5;
pub const OLD_FADE_SECS: f32 = 0.35;
/// Fading out: the dictation's when the recording ends, the chat's when
/// the answer is in.
pub const CARET_OUT_SECS: f32 = 0.3;
pub const CHAT_OUT_SECS: f32 = 0.7;
/// 30 frames a second while it moves; 10 once no word came for
/// [`IDLE_AFTER`] (the breathing is slow); none when it is gone.
pub const MOVING_FRAME: Duration = Duration::from_millis(33);
pub const IDLE_FRAME: Duration = Duration::from_millis(100);
const IDLE_AFTER: Duration = Duration::from_secs(1);

/// The frame interval for a wave whose text last moved `since` ago.
pub fn frame(since: Duration) -> Duration {
    if since < IDLE_AFTER {
        MOVING_FRAME
    } else {
        IDLE_FRAME
    }
}

/// Fading in over `secs` from the start, out over `out` after the end
/// (`after`: seconds since it ended, negative while it runs).
pub fn strength(run: f32, secs: f32, after: f32, out: f32) -> f32 {
    let fin = smooth(run / secs);
    if after < 0.0 {
        fin
    } else {
        fin * (1.0 - ease(after / out))
    }
}

/// The tip: a first-order lag into a critically damped spring, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Follow {
    lag: (f32, f32),
    pub x: f32,
    pub y: f32,
    v: (f32, f32),
}

impl Follow {
    /// At rest on `p`.
    pub fn at((x, y): (f32, f32)) -> Follow {
        Follow {
            lag: (x, y),
            x,
            y,
            v: (0.0, 0.0),
        }
    }

    /// `dt` seconds toward `target`; a long frame (a stall) runs at most a
    /// quarter of a second.
    pub fn step(&mut self, (tx, ty): (f32, f32), dt: f32) {
        let dt = dt.clamp(0.0, 0.25);
        let n = (dt / STEP).ceil().max(1.0);
        let h = dt / n;
        let la = 1.0 - (-h / LAG).exp();
        for _ in 0..n as u32 {
            self.lag.0 += (tx - self.lag.0) * la;
            self.lag.1 += (ty - self.lag.1) * la;
            self.v.0 += (WX * WX * (self.lag.0 - self.x) - 2.0 * WX * self.v.0) * h;
            self.v.1 += (WY * WY * (self.lag.1 - self.y) - 2.0 * WY * self.v.1) * h;
            self.x += self.v.0 * h;
            self.y += self.v.1 * h;
        }
    }

    /// Onto another line at once (the overlay moved with it); across, it glides on.
    pub fn drop_to(&mut self, y: f32) {
        (self.y, self.lag.1, self.v.1) = (y, y, 0.0);
    }

    /// Within a tenth of a point of `target`, and still.
    #[cfg(test)]
    fn settled(&self, (tx, ty): (f32, f32)) -> bool {
        let near = (self.x - tx).abs() < 0.1 && (self.y - ty).abs() < 0.1;
        near && self.v.0.abs() < 0.5 && self.v.1.abs() < 0.5
    }
}

/// Where the chat's text ended, as it came in: its speed for the lead, and
/// where it was a moment ago for the trail over fresh text.
#[derive(Debug, Clone, Default)]
pub struct Heads {
    /// (seconds, x, y, x with the lines laid end to end), oldest first.
    seen: VecDeque<(f32, f32, f32, f32)>,
}

impl Heads {
    /// The head at `t` is at `(x, y)`, `unwrapped` along all lines.
    pub fn push(&mut self, t: f32, (x, y): (f32, f32), unwrapped: f32) {
        if self.seen.back().is_some_and(|l| (l.1, l.2) == (x, y)) {
            return;
        }
        self.seen.push_back((t, x, y, unwrapped));
        while self.seen.len() > 2 && self.seen[1].0 < t - 1.0 {
            self.seen.pop_front();
        }
    }

    /// The last time it moved.
    pub fn moved(&self) -> Option<f32> {
        self.seen.back().map(|s| s.0)
    }

    fn unwrapped_at(&self, t: f32) -> Option<f32> {
        let first = self.seen.front()?;
        let at = self.seen.iter().rev().find(|s| s.0 <= t).unwrap_or(first);
        Some(at.3)
    }

    /// Points a second, along the lines, over the last [`SPEED_SECS`].
    pub fn speed(&self, t: f32) -> f32 {
        match (self.unwrapped_at(t), self.unwrapped_at(t - SPEED_SECS)) {
            (Some(now), Some(then)) => ((now - then) / SPEED_SECS).max(0.0),
            _ => 0.0,
        }
    }

    /// Where the head was at `t`: gliding from each place to the next over
    /// the time between them, so the trail spreads along the fresh words.
    pub fn at(&self, t: f32) -> Option<(f32, f32)> {
        let i = self.seen.iter().rposition(|s| s.0 <= t)?;
        let (t0, x0, y0, _) = self.seen[i];
        let Some(&(t1, x1, y1, _)) = self.seen.get(i + 1) else {
            return Some((x0, y0));
        };
        if y1 != y0 || t1 <= t0 {
            return Some((x1, y1));
        }
        let u = (t - t0) / (t1 - t0);
        Some((x0 + (x1 - x0) * u, y0))
    }
}

/// The text's baseline sits this share down an Accessibility line box
/// (ascent over ascent + descent for text fonts).
const ASCENT: f64 = 0.8;
/// The stream runs this far under the baseline.
pub const UNDER: f64 = 5.0;
/// The overlay: this high round the stream, this far left of the line's
/// start, this much room right of the caret, and wider in steps of
/// [`WIDEN`] so typing does not resize it with every word.
const HALF: f64 = 18.0;
const PAD: f64 = 24.0;
const AHEAD: f64 = 110.0;
const WIDEN: f64 = 160.0;
/// The tail at most this long; this long when the line's start is unknown.
const TAIL_MAX: f64 = 600.0;
const TAIL_GUESS: f64 = 220.0;
/// The tip sits this far right of the caret.
pub const PAST_CARET: f32 = 7.0;

/// Where the dictation's wave goes for a caret line, in screen points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spot {
    /// The overlay window.
    pub frame: Rect,
    /// The stream's line, the tail's start and the caret.
    pub y: f64,
    pub left: f64,
    pub caret: f64,
    /// The text's bottom: nothing is drawn over the line left of the caret above it.
    pub text_bottom: f64,
}

/// The wave's place for `line`: the caret's line from its start to the
/// caret ([`crate::platform::Platform::caret`]).
pub fn spot(line: &Rect) -> Spot {
    let caret = line.x + line.w;
    let y = line.y + line.h * ASCENT + UNDER;
    let start = if line.w > 0.0 {
        line.x.max(caret - TAIL_MAX)
    } else {
        caret - TAIL_GUESS
    };
    let x = ((start - PAD) / WIDEN).floor() * WIDEN;
    let w = ((caret + AHEAD - x) / WIDEN).ceil() * WIDEN;
    let frame = Rect {
        x,
        y: (y - HALF).round(),
        w,
        h: 2.0 * HALF,
    };
    Spot {
        frame,
        y,
        left: start - 6.0,
        caret,
        text_bottom: line.y + line.h,
    }
}

#[cfg(test)]
mod tests;
