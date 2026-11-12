//! The sidebar's room: how tall the history lists may grow beside the
//! chats, and the edge between them that a drag moves.

use super::super::style::use_palette;
use crate::panel::Panel;
use freya::prelude::*;
use std::{cell::Cell, rc::Rc, sync::Arc};

/// The edge's height: its line in the middle, the rest to take a press.
const EDGE: f32 = 9.0;

/// The rule over History: drag it up for more History, down for more
/// chats; the share is kept in state.json when the drag ends. Inert while
/// no History list shows.
#[derive(PartialEq)]
pub(super) struct Edge {
    /// History's share while dragged, None otherwise; the sidebar reads it.
    pub drag: State<Option<f32>>,
    /// The History lists' heights as laid out, in points, and which show.
    pub laid: Rc<[Cell<f32>; 2]>,
    pub shows: [bool; 2],
    /// Each list's cap now.
    pub cap: f32,
}

impl Component for Edge {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        // the press's height, the cap then, the open lists' heights then
        let mut from = use_state(|| None::<(f64, f32, [f32; 2], usize)>);
        let mut hover = use_state(|| false);
        let (mut drag, laid, shows, cap) = (self.drag, self.laid.clone(), self.shows, self.cap);
        let open = shows.iter().filter(|s| **s).count();
        let down = move |e: Event<PointerEventData>| {
            if open > 0 && e.data().is_primary() {
                e.stop_propagation();
                let mut heights = [0.0; 2];
                let shown = (0..2).filter(|i| shows[*i]);
                for (k, i) in shown.enumerate() {
                    heights[k] = laid[i].get();
                }
                from.set(Some((e.global_location().y, cap, heights, open)));
            }
        };
        // a nudge below this keeps the share as it was
        const NUDGE: f64 = 3.0;
        let moved = move |e: Event<PointerEventData>| {
            if let Some((y, cap, heights, n)) = from() {
                let up = y - e.global_location().y;
                if up.abs() < NUDGE && drag.peek().is_none() {
                    return;
                }
                let h = super::super::window_size().height.max(1.0);
                let to = cap_for(&heights[..n], cap, up as f32);
                drag.set(Some((to / h).clamp(0.0, 1.0)));
            }
        };
        let up = move |_: Event<PointerEventData>| {
            if from().is_none() {
                return;
            }
            from.set(None);
            if let Some(share) = drag() {
                drag.set(None);
                panel.set_history_share(share.max(0.001)); // 0 is "never dragged"
            }
        };
        let lit = open > 0 && (hover() || from().is_some());
        rect()
            .width(Size::fill())
            .height(Size::px(EDGE))
            .main_align(Alignment::Center)
            .maybe(open > 0, |r| r.cursor(CursorIcon::RowResize))
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            .on_pointer_down(down)
            .on_global_pointer_move(moved)
            .on_global_pointer_up(up)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(if lit { 2.0 } else { 1.0 }))
                    .background(if lit { p.accent } else { p.line }),
            )
    }
}

/// The History lists' cap (as kept, or as the edge is dragged), their
/// heights as laid out, and the edge over them; `shows`: which lists show.
pub(super) fn use_room(
    shows: [bool; 2],
    chats_open: bool,
    kept: Option<f32>,
) -> (f32, Rc<[Cell<f32>; 2]>, Edge) {
    let drag = use_state(|| None::<f32>);
    let laid = use_hook(|| Rc::new([Cell::new(0.0), Cell::new(0.0)]));
    let open = shows.iter().filter(|s| **s).count();
    let h = super::super::window_size().height;
    let cap = caps(h, open, chats_open, drag().or(kept));
    let edge = Edge {
        drag,
        laid: laid.clone(),
        shows,
        cap,
    };
    (cap, laid, edge)
}

/// The window's header, the search, three headings, the edge and a rule, with a
/// little slack: what the lists cannot have (measured in a 300 pt window).
pub(super) const FIXED: f32 = 205.0;
/// The chats keep this much while they show, or this share of a short window.
pub(super) const CHATS_MIN: f32 = 160.0;
/// A drag of the edge leaves the chats this much: two rows.
pub(super) const CHATS_DRAGGED: f32 = 56.0;
const CHATS_SHARE: f32 = 0.35;
/// A recent list takes at most this share of the window.
pub(super) const RECENT_SHARE: f32 = 0.22;

/// The most each open recent list may take of a window `h` tall: its share
/// (`dragged`, or [`RECENT_SHARE`]), less when the chats need the room,
/// nothing but its heading in a window too short for both (the chats'
/// heading never leaves it); all the room shared when the chats are folded.
pub(super) fn caps(h: f32, open_recent: usize, chats_open: bool, dragged: Option<f32>) -> f32 {
    let chats = match (chats_open, dragged) {
        (false, _) => 0.0,
        (true, None) => CHATS_MIN.min(h * CHATS_SHARE),
        (true, Some(_)) => CHATS_DRAGGED,
    };
    let each = ((h - FIXED - chats) / open_recent.max(1) as f32).max(0.0);
    match dragged.filter(|_| chats_open) {
        Some(share) => (h * share).min(each),
        None if chats_open => (h * RECENT_SHARE).min(each),
        None => each,
    }
}

/// The cap that moves the edge by `up` points from where the lists were
/// laid out under `cap`, at `heights`: a list shorter than the cap keeps
/// its height until the cap comes down to it, so the edge follows the
/// pointer, never runs ahead of it. Up from a short list starts at the cap.
pub(super) fn cap_for(heights: &[f32], cap: f32, up: f32) -> f32 {
    let short = |l: f32| l < cap - 1.0;
    // a list at the cap has more rows than it shows: it would grow on
    let laid_at = |c: f32| -> f32 {
        heights
            .iter()
            .map(|&l| if short(l) { c.min(l) } else { c })
            .sum()
    };
    let target = laid_at(cap) + up;
    if heights.iter().all(|l| short(*l)) {
        // nothing at the cap: up shows nothing more until rows come
        return if up >= 0.0 {
            cap + up
        } else {
            solve(laid_at, target, cap)
        };
    }
    solve(laid_at, target, cap.max(0.0) + up.max(0.0) + 1.0)
}

/// The `c` in 0..`hi` where `laid_at(c)` (rising) reaches `target`.
fn solve(laid_at: impl Fn(f32) -> f32, target: f32, hi: f32) -> f32 {
    let (mut lo, mut hi) = (0.0_f32, hi.max(0.0));
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if laid_at(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    hi
}

/// A history list at most `cap` points tall, its height noted in `laid`
/// (for the edge's drag). In pixels: ScrollView puts its max_height on its
/// content too, where a percentage is of the capped outer.
pub(super) fn list(rows: Vec<Element>, cap: f32, laid: (Rc<[Cell<f32>; 2]>, usize)) -> ScrollView {
    ScrollView::new()
        .width(Size::fill())
        .height(Size::auto())
        .max_height(Size::px(cap))
        .on_sized(move |e: Event<SizedEventData>| laid.0[laid.1].set(e.area.height()))
        .child(clear_of_bar(rows))
}

/// The scrollbar's room: it lies over the rows' right end (the counts).
const BAR: f32 = 14.0;

/// `rows` short of the scrollbar.
pub(super) fn clear_of_bar(rows: Vec<Element>) -> Rect {
    rect()
        .width(Size::fill())
        .padding((0.0, BAR, 0.0, 0.0))
        .children(rows)
}
