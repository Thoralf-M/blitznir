//! The panel's zoom (a double click on its header): where the window goes
//! and the frames on the way there. The window moves itself frame by frame
//! so each one is laid out and drawn at its size; AppKit's own zoom
//! stretches a picture of the window and lays it out only at the end.

use crate::platform::Rect;

/// How long the zoom takes, as AppKit's own takes about.
pub const SECS: f32 = 0.22;
/// Two frames this close (points, on every edge) count as the same.
const SAME: f64 = 4.0;

fn same(a: Rect, b: Rect) -> bool {
    (a.x - b.x).abs() <= SAME
        && (a.y - b.y).abs() <= SAME
        && (a.w - b.w).abs() <= SAME
        && (a.h - b.h).abs() <= SAME
}

/// Where a zoom from `now` goes, and the frame to come back to: a window
/// filling the screen's free area (`visible`) goes back to `before`, any
/// other fills it and remembers where it was. Already full and nothing to
/// go back to: it stays.
pub fn target(now: Rect, visible: Rect, before: Option<Rect>) -> (Rect, Option<Rect>) {
    if same(now, visible) {
        (before.unwrap_or(now), None)
    } else {
        (visible, Some(now))
    }
}

/// The frame at `t` (0 to 1, already eased) of the way from `from` to `to`,
/// on whole points so the text does not shimmer between them.
pub fn between(from: Rect, to: Rect, t: f32) -> Rect {
    let t = f64::from(t.clamp(0.0, 1.0));
    let at = |a: f64, b: f64| (a + (b - a) * t).round();
    Rect {
        x: at(from.x, to.x),
        y: at(from.y, to.y),
        w: at(from.w, to.w),
        h: at(from.h, to.h),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: Rect = Rect {
        x: 300.0,
        y: 200.0,
        w: 760.0,
        h: 540.0,
    };
    const FREE: Rect = Rect {
        x: 0.0,
        y: 33.0,
        w: 1728.0,
        h: 1084.0,
    };

    #[test]
    fn a_small_window_fills_the_free_area_and_comes_back() {
        let (to, back) = target(SMALL, FREE, None);
        assert_eq!((to, back), (FREE, Some(SMALL)));
        let (to, back) = target(FREE, FREE, back);
        assert_eq!((to, back), (SMALL, None), "back where it was");
    }

    #[test]
    fn a_window_resized_by_hand_zooms_out_again_not_back() {
        let moved = Rect { x: 10.0, ..SMALL };
        assert_eq!(target(moved, FREE, Some(SMALL)), (FREE, Some(moved)));
        // nearly full counts as full: macOS rounds a frame by a point or two
        let nearly = Rect {
            w: FREE.w - 2.0,
            ..FREE
        };
        assert_eq!(target(nearly, FREE, Some(SMALL)).0, SMALL);
        assert_eq!(target(FREE, FREE, None).0, FREE, "nothing to go back to");
    }

    #[test]
    fn the_frames_run_from_one_to_the_other_on_whole_points() {
        assert_eq!(between(SMALL, FREE, 0.0), SMALL);
        assert_eq!(between(SMALL, FREE, 1.0), FREE);
        let mid = between(SMALL, FREE, 0.5);
        assert_eq!(
            mid,
            Rect {
                x: 150.0,
                y: 117.0,
                w: 1244.0,
                h: 812.0
            }
        );
        assert_eq!(between(SMALL, FREE, 2.0), FREE, "clamped");
    }
}
