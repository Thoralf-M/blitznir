//! Where lightning happens, for both shells' zap: the screen the overlay
//! covers, the rect the bolts land in, and the menu-bar hammer they start
//! from. Rects come in in [`Rect`]'s desktop space and go out in the
//! overlay's: its own top left is 0, 0, in points.

use crate::platform::{Bounds, Rect};

/// A target larger than this is a field or a window, not the selected text.
const TEXT_MAX_W: f64 = 1400.0;
const TEXT_MAX_H: f64 = 240.0;
/// Where the bolts land when the target is not the text itself.
const PATCH: (f64, f64) = (80.0, 16.0);

/// tray-icon draws the icon this many points tall, and as wide: it is square.
const ICON_PT: f64 = 18.0;
/// The icon's left edge from the status item's, in points (measured on
/// macOS 26: item at x 3049, glyph from 3063, which is 3.5 pt into the icon).
const ICON_INSET_PT: f64 = 10.0;
/// Where a bolt leaves the hammer, in fractions of the icon: the centre of
/// the head's lower edge, from hammer.svg's geometry.
const FACE: (f64, f64) = (0.35, 0.52);

/// Where the bolts land, and whether that is the selected text itself: the
/// text however large (several lines), or a field small enough to be it. A
/// larger field or a window: the mouse inside it, which just made the
/// selection, is a far better guess than its centre. No rect: the mouse.
/// Neither (Linux: no compositor says): None, the overlay's middle.
pub fn aim(sel: Option<Bounds>, mouse: Option<(f64, f64)>) -> Option<(Rect, bool)> {
    let small = |r: &Rect| r.w <= TEXT_MAX_W && r.h <= TEXT_MAX_H;
    match (sel.map(|b| (b.rect, b.text)), mouse) {
        (Some((r, text)), _) if text || small(&r) => Some((r, true)),
        (Some((r, _)), Some(m)) if r.contains(m) => Some((patch(m), false)),
        (Some((r, _)), _) => Some((patch(r.centre()), false)),
        (None, Some(m)) => Some((patch(m), false)),
        (None, None) => None,
    }
}

fn patch((x, y): (f64, f64)) -> Rect {
    let (w, h) = PATCH;
    Rect {
        x: x - w / 2.0,
        y: y - h / 2.0,
        w,
        h,
    }
}

/// The screen that holds `p`, else the main one.
pub fn screen_at(screens: &[(f64, Rect)], p: (f64, f64), main: Rect) -> Rect {
    let on = screens.iter().map(|(_, a)| *a).find(|a| a.contains(p));
    on.unwrap_or(main)
}

pub struct Sky {
    /// The screen the overlay covers.
    pub area: Rect,
    /// Every screen with its scale, to read the tray's position back.
    screens: Vec<(f64, Rect)>,
    /// Where the bolts land, in the overlay's points.
    pub aim: Rect,
    /// `aim` is the selected text: the overlay lights it up on every strike.
    pub exact: bool,
}

impl Sky {
    /// `target` ([`aim`]) moves from the desktop onto the overlay over
    /// `area`. None, or one not on it (Linux: the mouse on another screen),
    /// becomes the patch at the overlay's centre.
    pub fn new(area: Rect, screens: Vec<(f64, Rect)>, target: Option<(Rect, bool)>) -> Sky {
        let (aim, exact) = match target {
            Some((aim, exact)) if area.contains(aim.centre()) => (
                Rect {
                    x: aim.x - area.x,
                    y: aim.y - area.y,
                    ..aim
                },
                exact,
            ),
            _ => (patch((area.w / 2.0, area.h / 2.0)), false),
        };
        Sky {
            area,
            screens,
            aim,
            exact,
        }
    }

    /// The hammer's face in the overlay's points, from the status item's
    /// top left `(x, y)` and height `h` in physical pixels of its own
    /// screen's scale (tray-icon's rect). Its screen is the first in
    /// `screens` that the scaled position lands on (the primary comes first:
    /// mirrored screens share one rect). On another screen than the
    /// overlay's, the same item shows on the overlay's own menu bar, as far
    /// from its right edge (macOS puts it on every one): the bolt starts
    /// there. None only when it would be off the overlay.
    pub fn source(&self, at: (f64, f64), h: f64) -> Option<(f64, f64)> {
        self.hammer(at, h).map(|(face, _)| face)
    }

    /// [`Sky::source`], and the status item's bottom edge, the menu bar's,
    /// in the overlay's y: the beam runs along under the bar.
    pub fn hammer(&self, (x, y): (f64, f64), h: f64) -> Option<((f64, f64), f64)> {
        // by the item's middle: its frame reaches past the screen's top
        // edge (y -2 px on macOS 26), its top left is on no screen
        let on = |(s, a): &&(f64, Rect)| a.contains((x / s, (y + h / 2.0) / s));
        let (s, screen) = self.screens.iter().find(on)?;
        let (x, y, h) = (x / s, y / s, h / s);
        let face = (
            x + ICON_INSET_PT + FACE.0 * ICON_PT,
            y + (h - ICON_PT) / 2.0 + FACE.1 * ICON_PT,
        );
        let below = face.1 + (h - ICON_PT) / 2.0 + (1.0 - FACE.1) * ICON_PT;
        if self.area.contains(face) {
            let top = self.area.y;
            return Some(((face.0 - self.area.x, face.1 - top), below - top));
        }
        let from_right = screen.x + screen.w - face.0;
        let here = (self.area.w - from_right, face.1 - screen.y);
        (0.0..=self.area.w)
            .contains(&here.0)
            .then_some((here, below - screen.y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn the_selected_text_is_the_aim_and_a_field_or_window_only_hints_at_it() {
        let text = |r| {
            Some(Bounds {
                rect: r,
                text: true,
            })
        };
        let hint = |r| {
            Some(Bounds {
                rect: r,
                text: false,
            })
        };
        let line = r(300.0, 200.0, 420.0, 17.0);
        assert_eq!(aim(text(line), Some((0.0, 0.0))), Some((line, true)));
        let lines = r(300.0, 200.0, 1544.0, 368.0);
        assert_eq!(
            aim(text(lines), Some((0.0, 0.0))),
            Some((lines, true)),
            "many lines"
        );
        let limit = r(0.0, 0.0, TEXT_MAX_W, TEXT_MAX_H);
        assert_eq!(
            aim(hint(limit), None),
            Some((limit, true)),
            "a field this small"
        );
        let field = r(100.0, 100.0, 1500.0, 900.0);
        let (patch, exact) = aim(hint(field), Some((640.0, 480.0))).unwrap();
        assert!(!exact);
        assert_eq!(patch, r(600.0, 472.0, 80.0, 16.0), "the mouse inside it");
        let (patch, _) = aim(hint(field), Some((3000.0, 480.0))).unwrap();
        assert_eq!(patch.centre(), field.centre(), "the mouse elsewhere");
        let (patch, _) = aim(hint(field), None).unwrap();
        assert_eq!(patch.centre(), field.centre(), "no mouse");
        let tall = r(100.0, 100.0, 300.0, 241.0);
        assert!(!aim(hint(tall), None).unwrap().1, "a text view, not a line");
        let at_mouse = Some((r(-30.0, 12.0, 80.0, 16.0), false));
        assert_eq!(aim(None, Some((10.0, 20.0))), at_mouse);
        assert_eq!(aim(None, None), None);
    }

    /// A 1512×982 Retina laptop as the main screen, a 2560×1440 display to
    /// its right with its top 458 pt higher, a 1920×1080 one to its left.
    fn desk() -> Vec<(f64, Rect)> {
        vec![
            (2.0, r(0.0, 0.0, 1512.0, 982.0)),
            (1.0, r(1512.0, -458.0, 2560.0, 1440.0)),
            (1.0, r(-1920.0, 0.0, 1920.0, 1080.0)),
        ]
    }

    #[test]
    fn the_overlay_covers_the_targets_screen_and_takes_it_into_its_own_points() {
        let screens = desk();
        let main = screens[0].1;
        let on_right = r(2000.0, -300.0, 300.0, 18.0);
        let area = screen_at(&screens, on_right.centre(), main);
        assert_eq!(area, screens[1].1);
        let sky = Sky::new(area, screens.clone(), Some((on_right, true)));
        assert_eq!(sky.aim, r(488.0, 158.0, 300.0, 18.0));
        assert!(sky.exact);
        let on_left = r(-1000.0, 500.0, 200.0, 16.0);
        let area = screen_at(&screens, on_left.centre(), main);
        let sky = Sky::new(area, screens.clone(), Some((on_left, true)));
        assert_eq!(sky.aim, r(920.0, 500.0, 200.0, 16.0));
        let between = (1512.0 - 1.0, 990.0); // below the laptop's bottom edge
        assert_eq!(screen_at(&screens, between, main), main, "on none: main");
    }

    #[test]
    fn an_aim_off_the_covered_screen_or_none_lands_in_its_middle_and_lights_nothing() {
        let screens = desk();
        let main = screens[0].1;
        let off = Some((r(3000.0, 10.0, 200.0, 16.0), true));
        let sky = Sky::new(main, screens.clone(), off);
        assert!(!sky.exact);
        assert_eq!(sky.aim.centre(), (756.0, 491.0));
        let sky = Sky::new(main, screens, None);
        assert_eq!((sky.aim.centre(), sky.exact), ((756.0, 491.0), false));
    }

    #[test]
    fn the_bolt_leaves_the_hammer_on_the_status_items_own_screen() {
        let screens = desk();
        let main = screens[0].1;
        let sky = Sky::new(main, screens.clone(), None);
        // an item at 1200, 0 pt on the Retina laptop, 24 pt tall: pixels ×2
        let (x, y) = sky.source((2400.0, 0.0), 48.0).unwrap();
        assert!((x - (1200.0 + ICON_INSET_PT + FACE.0 * ICON_PT)).abs() < 1e-9);
        assert!((y - (3.0 + FACE.1 * ICON_PT)).abs() < 1e-9);
        let (_, bar) = sky.hammer((2400.0, 0.0), 48.0).unwrap();
        assert!((bar - 24.0).abs() < 1e-9, "the item's bottom: {bar}");
        let right = Sky::new(screens[1].1, screens.clone(), None);
        // on the laptop's menu bar, the overlay on the right screen: the
        // same item on the right screen's bar, as far from its right edge
        let (x, y) = right.source((2400.0, 0.0), 48.0).unwrap();
        let from_right = 1512.0 - (1200.0 + ICON_INSET_PT + FACE.0 * ICON_PT);
        assert!((x - (2560.0 - from_right)).abs() < 1e-9, "{x}");
        assert!((y - (3.0 + FACE.1 * ICON_PT)).abs() < 1e-9);
        let (_, bar) = right.hammer((2400.0, 0.0), 48.0).unwrap();
        assert!((bar - 24.0).abs() < 1e-9, "on this bar too: {bar}");
        let (x, _) = right.source((3000.0, -458.0), 24.0).unwrap();
        assert!((x - (3000.0 - 1512.0 + ICON_INSET_PT + FACE.0 * ICON_PT)).abs() < 1e-9);
    }

    #[test]
    fn an_item_reaching_above_the_screens_top_edge_is_still_on_it() {
        // a real log: a 3840x1600 screen at 1x, the item at y -2, 33 px tall
        let wide = r(0.0, 0.0, 3840.0, 1600.0);
        let sky = Sky::new(wide, vec![(1.0, wide)], None);
        let (x, y) = sky.source((3221.0, -2.0), 33.0).expect("on the screen");
        assert!((x - (3221.0 + ICON_INSET_PT + FACE.0 * ICON_PT)).abs() < 1e-9);
        assert!((0.0..33.0).contains(&y), "on the menu bar: {y}");
    }

    #[test]
    fn on_mirrored_screens_the_primarys_scale_places_the_hammer() {
        // an ultrawide at 1x mirrored onto a Retina laptop at 2x: one rect
        let wide = r(0.0, 0.0, 3840.0, 1600.0);
        let screens = vec![(1.0, wide), (2.0, wide)];
        let sky = Sky::new(wide, screens, None);
        // the item at 3500 pt on the primary, in its 1x pixels
        let (x, _) = sky.source((3500.0, 0.0), 24.0).unwrap();
        assert!(
            (x - (3500.0 + ICON_INSET_PT + FACE.0 * ICON_PT)).abs() < 1e-9,
            "not halved: {x}"
        );
    }
}
