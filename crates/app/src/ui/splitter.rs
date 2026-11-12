//! The edge between the sidebar and the chat: drag it to make the sidebar
//! wider or narrower, or almost shut to fold it away. Folded, it is a grip
//! at the window's left edge, lit under the mouse: drag it out and the
//! sidebar follows. The width is kept in state.json when the drag ends,
//! not on every move.

use super::{Peek, style::use_palette};
use crate::panel::Panel;
use blitznir_core::store::State as Saved;
use freya::prelude::*;
use std::sync::Arc;

/// Let go below this width and the sidebar folds instead.
const FOLD_BELOW: f32 = 100.0;

/// The drawn width of a drag's raw one.
pub fn shown(raw: f32) -> f32 {
    raw.clamp(Saved::SIDE_MIN, Saved::SIDE_MAX)
}

/// The sidebar and its edge: as wide as `width`, or as the drag in
/// progress; Settings' index (search, sections, About) while `settings`
/// is open. Folded (not `open`), only the grip, and the sidebar while it is
/// dragged out.
pub fn side(
    open: bool,
    (width, settings): (f32, bool),
    drag: State<Option<f32>>,
    narrow: bool,
) -> Vec<Element> {
    let dragged_out = drag().is_some_and(|raw| raw >= FOLD_BELOW);
    let shown_width = drag().map_or(width, shown);
    // always two children, so the grip keeps its drag when the bar appears
    let bar = if !open && !dragged_out {
        rect().into_element()
    } else if settings {
        super::form_nav::FormIndex { width: shown_width }.into_element()
    } else {
        super::sidebar::Sidebar { width: shown_width }.into_element()
    };
    let edge = Splitter {
        drag,
        width: if open { width } else { 0.0 },
        narrow,
        folded: !open,
    };
    vec![bar, edge.into_element()]
}

#[derive(PartialEq)]
struct Splitter {
    /// The raw width while dragged, None otherwise; the window reads it.
    pub drag: State<Option<f32>>,
    /// The width the drag starts from.
    pub width: f32,
    /// The window is narrow: the sidebar is peeked, not unfolded.
    pub narrow: bool,
    /// The sidebar is folded: this is the grip that drags it out.
    pub folded: bool,
}

impl Component for Splitter {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let Peek(mut peek) = use_consume::<Peek>();
        let mut from = use_state(|| None::<(f64, f32)>);
        let mut hover = use_state(|| false);
        let (mut drag, width, narrow, folded) = (self.drag, self.width, self.narrow, self.folded);
        let down = move |e: Event<PointerEventData>| {
            if e.data().is_primary() {
                e.stop_propagation();
                from.set(Some((e.global_location().x, width)));
            }
        };
        let moved = move |e: Event<PointerEventData>| {
            if let Some((x, w)) = from() {
                drag.set(Some(w + (e.global_location().x - x) as f32));
            }
        };
        let up = move |_: Event<PointerEventData>| {
            if from().is_none() {
                return;
            }
            from.set(None);
            let Some(raw) = drag() else { return };
            drag.set(None);
            if raw >= FOLD_BELOW {
                panel.set_side_width(shown(raw));
                if folded && narrow {
                    peek.set(true);
                } else if folded {
                    panel.set_folded(false);
                }
            } else if folded {
                // not out far enough: it stays folded
            } else if narrow {
                peek.set(false);
            } else {
                panel.set_folded(true);
            }
        };
        let lit = hover() || from().is_some();
        // folded and not under the mouse: there, but not drawn
        let line = if lit {
            p.accent
        } else if folded {
            Color::TRANSPARENT
        } else {
            p.line
        };
        rect()
            .width(Size::px(if folded { 6.0 } else { 5.0 }))
            .height(Size::fill())
            .horizontal()
            .main_align(Alignment::Center)
            .cursor(CursorIcon::ColResize)
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            .on_pointer_down(down)
            .on_global_pointer_move(moved)
            .on_global_pointer_up(up)
            .child(
                rect()
                    .width(Size::px(if lit { 2.0 } else { 1.0 }))
                    .height(Size::fill())
                    .background(line),
            )
    }
}
