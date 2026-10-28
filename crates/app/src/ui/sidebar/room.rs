//! The sidebar's room: how tall the history lists may grow beside the chats.

use super::super::style::Palette;
use freya::prelude::*;

/// The room above the rule between two lists.
const GAP: f32 = 16.0;

/// The room and the rule between two lists.
pub(super) fn rule(p: &Palette) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::px(1.0))
        .margin((GAP, 0.0, 0.0, 0.0))
        .background(p.line)
}

/// The window's header, the search, three headings and two rules, with a
/// little slack: what the lists cannot have (measured in a 300 pt window).
pub(super) const FIXED: f32 = 205.0;
/// The chats keep this much while they show, or this share of a short window.
pub(super) const CHATS_MIN: f32 = 160.0;
const CHATS_SHARE: f32 = 0.35;
/// A recent list takes at most this share of the window.
pub(super) const RECENT_SHARE: f32 = 0.22;

/// The most each open recent list may take of a window `h` tall: its share,
/// less when the chats need the room, nothing but its heading in a window
/// too short for both (the chats' heading never leaves it); all the room
/// shared when the chats are folded.
pub(super) fn caps(h: f32, open_recent: usize, chats_open: bool) -> f32 {
    let chats = if chats_open {
        CHATS_MIN.min(h * CHATS_SHARE)
    } else {
        0.0
    };
    let each = ((h - FIXED - chats) / open_recent.max(1) as f32).max(0.0);
    if chats_open {
        (h * RECENT_SHARE).min(each)
    } else {
        each
    }
}

/// A history list at most `cap` points tall. In pixels: ScrollView puts its
/// max_height on its content too, where a percentage is of the capped outer.
pub(super) fn list(rows: Vec<Element>, cap: f32) -> ScrollView {
    ScrollView::new()
        .width(Size::fill())
        .height(Size::auto())
        .max_height(Size::px(cap))
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
