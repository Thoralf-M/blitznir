//! The sidebar's room: how tall the history lists may grow beside the
//! chats, and the edge between them that a drag moves.

use super::super::style::use_palette;
use crate::panel::Panel;
use freya::prelude::*;
use std::{cell::Cell, rc::Rc, sync::Arc};

/// The edge's height: its line in the middle, the rest to take a press.
const EDGE: f32 = 9.0;

/// The rule over History: drag it up for more History, down for more
/// chats. Once dragged, History is a block of that height (a share of the
/// window, kept in state.json when the drag ends) that folding or a search
/// leaves where it is; the edge follows the pointer, between one row of
/// each open list and one row of the chats.
#[derive(PartialEq)]
pub(super) struct Edge {
    /// History's share while dragged, None otherwise; the sidebar reads it.
    pub drag: State<Option<f32>>,
    pub laid: Rc<Laid>,
    /// The least the block may be, and the most (the chats keep a row).
    pub bounds: (f32, f32),
}

impl Component for Edge {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        // the press's height and the block's then
        let mut from = use_state(|| None::<(f64, f32)>);
        let mut hover = use_state(|| false);
        let (mut drag, laid, (floor, most)) = (self.drag, self.laid.clone(), self.bounds);
        let down = move |e: Event<PointerEventData>| {
            if e.data().is_primary() {
                e.stop_propagation();
                from.set(Some((e.global_location().y, laid.block.get())));
            }
        };
        // a nudge below this keeps the share as it was
        const NUDGE: f64 = 3.0;
        let moved = move |e: Event<PointerEventData>| {
            if let Some((y, block)) = from() {
                let up = y - e.global_location().y;
                if up.abs() < NUDGE && drag.peek().is_none() {
                    return;
                }
                let h = super::super::window_size().height.max(1.0);
                let to = (block + up as f32).clamp(floor, most.max(floor));
                drag.set(Some(to / h));
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
        let lit = hover() || from().is_some();
        rect()
            .width(Size::fill())
            .height(Size::px(EDGE))
            .main_align(Alignment::Center)
            .cursor(CursorIcon::RowResize)
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

/// What the last frame laid out, for the next: the History lists, History's
/// block, the chats' list, and the block's content (headings and lists).
#[derive(Default, PartialEq)]
pub(super) struct Laid {
    pub lists: [Cell<f32>; 2],
    pub block: Cell<f32>,
    pub chats: Cell<f32>,
    pub content: Cell<f32>,
    /// The cap and which lists showed: a list just opened was not laid out.
    last: Cell<(f32, [bool; 2])>,
}

/// The sidebar's room: each History list's cap, what was laid out, the
/// edge, and History's block height once dragged (None: as tall as it is).
pub(super) struct Room {
    pub cap: f32,
    pub laid: Rc<Laid>,
    pub edge: Edge,
    pub block: Option<f32>,
}

/// The block's headings and padding, from the last frame: its content
/// less the lists that `showed` in it (not the ones that show now: a list
/// folded since was still in that content).
pub(super) fn chrome(content: f32, lists: [f32; 2], showed: [bool; 2]) -> f32 {
    let laid: f32 = (0..2).filter(|i| showed[*i]).map(|i| lists[i]).sum();
    (content - laid).max(0.0)
}

/// A row of a list: what an opened list shows at least (a replacement's,
/// its original above, is the taller).
const ROW_MIN: f32 = 40.0;

/// The room for `shows`' lists beside the chats, History's share as kept,
/// or as the edge is dragged.
pub(super) fn use_room(shows: [bool; 2], chats_open: bool, kept: Option<f32>) -> Room {
    let panel = use_consume::<Arc<Panel>>();
    let drag = use_state(|| None::<f32>);
    let laid = use_hook(|| Rc::new(Laid::default()));
    let open = shows.iter().filter(|s| **s).count();
    let h = super::super::window_size().height;
    let (was, showed) = laid.last.get();
    let shown = |i: &usize| shows[*i] && showed[*i];
    let short: Vec<f32> = (0..2)
        .filter(|i| shown(i) && laid.lists[*i].get() < was - 1.0)
        .map(|i| laid.lists[i].get())
        .collect();
    let lists = [0, 1].map(|i| laid.lists[i].get());
    let chrome = chrome(laid.content.get(), lists, showed);
    let floor = chrome + ROW_MIN * open as f32;
    let keep = if chats_open { CHAT_ROW } else { 0.0 };
    let measured = laid.block.get() > 0.0;
    let most = match measured {
        true => laid.block.get() + laid.chats.get() - keep,
        false => f32::MAX,
    };
    let (cap, block) = match drag().or(kept) {
        None => (caps(h, open, chats_open, &short), None),
        Some(share) => {
            let block = (share * h).clamp(floor, most.max(floor));
            // a list opened with no room: the edge rises to show a row, and stays
            if measured && drag().is_none() && block > share * h + 0.5 {
                panel.set_history_share(block / h);
            }
            let each = shared((block - chrome).max(0.0), open.max(1), &short);
            (each, Some(block))
        }
    };
    laid.last.set((cap, shows));
    let edge = Edge {
        drag,
        laid: laid.clone(),
        bounds: (floor, most),
    };
    Room {
        cap,
        laid,
        edge,
        block,
    }
}

/// The window's header, the search, three headings, the edge and a rule, with a
/// little slack: what the lists cannot have (measured in a 300 pt window).
pub(super) const FIXED: f32 = 178.0;
/// The chats keep this much while they show, or this share of a short window.
pub(super) const CHATS_MIN: f32 = 160.0;
/// A dragged edge leaves the chats a row.
const CHAT_ROW: f32 = 28.0;
const CHATS_SHARE: f32 = 0.35;
/// A recent list takes at most this share of the window.
pub(super) const RECENT_SHARE: f32 = 0.22;

/// The most each open recent list may take of a window `h` tall, the edge
/// never dragged: its share ([`RECENT_SHARE`]), less when the chats need
/// the room, nothing but its heading in a window too short for both (the
/// chats' heading never leaves it); all the room shared when the chats are
/// folded. `short`: the heights of open lists with fewer rows than that,
/// which leave the rest of their part of the room to the others.
pub(super) fn caps(h: f32, open_recent: usize, chats_open: bool, short: &[f32]) -> f32 {
    let chats = if chats_open {
        CHATS_MIN.min(h * CHATS_SHARE)
    } else {
        0.0
    };
    let each = shared((h - FIXED - chats).max(0.0), open_recent.max(1), short);
    if chats_open {
        (h * RECENT_SHARE).min(each)
    } else {
        each
    }
}

/// The most each of `n` lists may take of `room` when the `short` ones keep
/// their own heights: a short list that fits its part leaves the rest of it.
pub(super) fn shared(room: f32, n: usize, short: &[f32]) -> f32 {
    let mut short = short.to_vec();
    short.sort_by(f32::total_cmp);
    let (mut left, mut n) = (room, n);
    for s in short {
        if n <= 1 || s * n as f32 > left {
            break;
        }
        (left, n) = (left - s, n - 1);
    }
    left / n as f32
}

/// A history list at most `cap` points tall, its height noted in `laid`
/// (for the edge's drag). In pixels: ScrollView puts its max_height on its
/// content too, where a percentage is of the capped outer.
pub(super) fn list(rows: Vec<Element>, cap: f32, laid: (Rc<Laid>, usize)) -> ScrollView {
    ScrollView::new()
        .width(Size::fill())
        .height(Size::auto())
        .max_height(Size::px(cap))
        .on_sized(move |e: Event<SizedEventData>| laid.0.lists[laid.1].set(e.area.height()))
        .child(clear_of_bar(rows))
}

/// The chats' list: it takes the room History leaves; its height noted
/// in `laid` (the most the edge may take from it).
pub(super) fn chats_list(rows: Vec<Element>, laid: &Rc<Laid>) -> ScrollView {
    let laid = laid.clone();
    ScrollView::new()
        .width(Size::fill())
        .height(Size::flex(1.0))
        .on_sized(move |e: Event<SizedEventData>| laid.chats.set(e.area.height()))
        .child(clear_of_bar(rows))
}

/// History's block: its `heading` and `lists`, as tall as they are or,
/// once the edge was dragged, `block` tall (the rest empty, so folding or
/// a search leaves the edge where it is). Both heights noted in `laid`.
pub(super) fn block(
    block: Option<f32>,
    laid: &Rc<Laid>,
    heading: Rect,
    lists: Option<Rect>,
) -> Rect {
    let (l1, l2) = (laid.clone(), laid.clone());
    let content = rect()
        .width(Size::fill())
        .on_sized(move |e: Event<SizedEventData>| l1.content.set(e.area.height()))
        .child(heading)
        .maybe_child(lists);
    match block {
        Some(b) => rect().height(Size::px(b)).overflow(Overflow::Clip),
        None => rect(),
    }
    .width(Size::fill())
    .on_sized(move |e: Event<SizedEventData>| l2.block.set(e.area.height()))
    .child(content)
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
