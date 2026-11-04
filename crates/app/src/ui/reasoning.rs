//! An answer's reasoning, above it: one muted "Thinking" row, folded until
//! a press opens it in a box of its own scroll, with a Fold at its end. The
//! fold is this view's own, per message, never stored; the band keeps the
//! streaming answer's, so it stays as it was when the answer is in.

use super::follow::{Follow, use_follow};
use super::icons::{self, icon};
use super::select::Selectable;
use super::style::{Look, Palette, button, head, pressable, use_palette};
use crate::panel::view::Ui;
use blitznir_core::store::Unthought;
use freya::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// An opened trace's box at most, in points; a longer one scrolls in it.
const BOX_MAX: f32 = 240.0;

#[derive(PartialEq)]
pub struct Reasoning {
    pub text: String,
    pub unthought: Unthought,
    /// The fold, when the band keeps it: the answer streaming, then just
    /// in. Its box follows its own bottom, as the band's does.
    pub fold: Option<State<bool>>,
}

/// The chevron and a word, pressed to fold or unfold.
fn toggle(
    p: &Palette,
    (chevron, title): (&'static [u8], &str),
    press: impl FnMut(Event<PressEventData>) + 'static,
) -> Rect {
    let row = rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(3.0)
        .child(icon(chevron, p.muted, 12.0))
        .child(head(p, title).font_size(12.0));
    pressable(
        button(p, Look::Tool).padding((1.0, 2.0)).child(row),
        EventHandler::new(press),
    )
}

impl Component for Reasoning {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let band = use_consume::<Follow>();
        let inner = use_follow();
        let own = use_state(|| false);
        let mut open = self.fold.unwrap_or(own);
        let shown = open();
        let title = match self.unthought {
            Unthought::No => "Thinking",
            Unthought::RanOut => "Thinking (ran out, answered without it)",
            Unthought::Looped => "Thinking (looped, answered without it)",
        };
        let flip = move |_| {
            band.hold();
            open.toggle();
        };
        let chevron = if shown { icons::OPEN } else { icons::SHUT };
        let top = toggle(&p, (chevron, title), flip.clone());
        let follows = self.fold.is_some().then_some("live");
        let body = shown.then(|| {
            let text = Selectable::new()
                .span(self.text.trim().to_string())
                .width(Size::fill())
                .font_size(12.0)
                .color(p.muted);
            let (grown, framed) = (inner.clone(), inner.clone());
            let lines = rect()
                .width(Size::fill())
                .padding((0.0, 10.0, 0.0, 17.0))
                .on_sized(move |e: Event<SizedEventData>| grown.content(e.area.height(), follows))
                .child(text);
            let scroll = ScrollView::new_controlled(inner.controller())
                .width(Size::fill())
                .height(Size::auto())
                .max_height(Size::px(BOX_MAX))
                .drag_scrolling(false)
                .on_sized(move |e: Event<SizedEventData>| framed.viewport(e.area))
                .child(lines);
            rect()
                .width(Size::fill())
                .spacing(2.0)
                .padding((0.0, 0.0, 6.0, 0.0))
                .child(scroll)
                .child(toggle(&p, (icons::OPEN, "Fold"), flip))
        });
        rect()
            .width(Size::fill())
            .spacing(4.0)
            .child(top)
            .maybe_child(body)
    }
}

/// The streaming answer's fold, and the message it lands as.
pub type Live = (State<bool>, Option<usize>);

/// [`Live`], folded again at each new answer, and gone with another chat;
/// the band's `follow` goes to its folds ([`Follow::hold`]).
pub fn use_live(state: State<Ui>, follow: Follow) -> Live {
    use_provide_context(|| follow);
    let mut fold = use_state(|| false);
    let at = use_hook(|| Rc::new(Cell::new(None::<usize>)));
    let (lands, shown) = (at.clone(), use_hook(|| Rc::new(RefCell::new(None))));
    let was = use_hook(|| Rc::new(Cell::new(false)));
    use_side_effect(move || {
        let u = state.read();
        let now = u.chatting.then(|| u.chat_id.clone());
        if shown.replace(now.clone()) != now {
            lands.set(None);
        }
        // each answer starts folded, a resent turn's too
        let streaming = u.streaming.is_some();
        if streaming && !was.replace(true) {
            lands.set(Some(u.messages.len()));
            fold.set(false);
        }
        was.set(streaming);
    });
    (fold, at.get())
}
