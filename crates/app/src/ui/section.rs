//! A list's heading in the sidebar, with its Delete all, asked once in the
//! heading row itself: no dialog. The Chats heading also starts a new chat.
//! History's heading folds dictations and replacements into one line.

use super::{
    icons::{self, icon, icon_button},
    style::{Look, Palette, button, head, pressable, use_palette},
    tip::Tip,
};
use crate::panel::{List, Panel, noun};
use freya::prelude::*;
use std::sync::Arc;

/// A small word button in a heading.
pub fn choice(
    p: &Palette,
    look: Look,
    text: &str,
    press: impl FnMut(Event<PressEventData>) + 'static,
) -> Rect {
    let b = button(p, look)
        .padding((0.0, 5.0))
        .child(label().text(text.to_string()).font_size(11.0));
    pressable(b, EventHandler::new(press))
}

/// History's heading: how many dictations and replacements it holds; a
/// press unfolds it or folds it back (state.json keeps it), unless a search
/// holds it open (`held`).
pub fn history_heading(
    p: &Palette,
    panel: &Arc<Panel>,
    (open, held): (bool, bool),
    n: usize,
) -> Rect {
    let chevron = if open { icons::OPEN } else { icons::SHUT };
    let heading = rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(3.0)
        .child(icon(chevron, p.muted, 12.0))
        .child(head(p, format!("History · {n}")).font_size(12.0));
    let fold = panel.clone();
    let b = button(p, Look::Tool).padding((1.0, 2.0)).child(heading);
    rect()
        .width(Size::fill())
        .padding((10.0, 1.0, 4.0, 6.0))
        .child(pressable(b, move |_| {
            if !held {
                fold.open_history(!open);
            }
        }))
}

/// A list's heading: its title, how many rows it shows, and a trash that
/// deletes all of the list once asked, in the heading itself. A press on
/// the title folds the list away or back (state.json keeps it).
#[derive(PartialEq)]
pub struct Section {
    pub kind: List,
    pub title: &'static str,
    pub shown: usize,
    /// All of the list, found or not: what Delete all takes.
    pub total: usize,
    /// Folded away: only this heading shows.
    pub closed: bool,
    /// Held open by a search: its title folds nothing (nothing would show).
    pub held: bool,
}

impl Component for Section {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let mut asking = use_state(|| false);
        let (kind, n) = (self.kind, self.total);
        let bar = rect()
            .width(Size::fill())
            .horizontal()
            .content(Content::flex())
            .cross_align(Alignment::Center)
            .spacing(4.0)
            .padding((10.0, 1.0, 4.0, 6.0));
        if asking() && n > 0 {
            let ask = head(&p, format!("Delete {n} {}?", noun(kind, n)))
                .font_size(11.0)
                .max_lines(1)
                .text_overflow(TextOverflow::Ellipsis);
            let yes = move |_| {
                asking.set(false);
                panel.clear(kind);
            };
            bar.child(rect().width(Size::flex(1.0)).child(ask))
                .child(choice(&p, Look::Plain, "Yes", yes))
                .child(choice(&p, Look::Tool, "No", move |_| asking.set(false)))
        } else {
            let chevron = if self.closed {
                icons::SHUT
            } else {
                icons::OPEN
            };
            let heading = rect()
                .horizontal()
                .cross_align(Alignment::Center)
                .spacing(3.0)
                .child(icon(chevron, p.muted, 12.0))
                .child(head(&p, format!("{} · {}", self.title, self.shown)).font_size(12.0));
            let fold = panel.clone();
            let b = button(&p, Look::Tool).padding((1.0, 2.0)).child(heading);
            let held = self.held;
            let title = pressable(b, move |_| {
                if !held {
                    fold.fold_list(kind);
                }
            });
            let trash = (n > 0).then(|| {
                let tip = format!("Delete all {n} {}", noun(kind, n));
                icon_button(&p, icons::TRASH, tip, true, move |_| asking.set(true))
            });
            let new = (kind == List::Chats).then(|| {
                let text = rect()
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .spacing(3.0)
                    .child(icon(icons::NEW, p.muted, 12.0))
                    .child(label().text("New").font_size(11.0));
                let b = button(&p, Look::Tool).padding((1.0, 5.0)).child(text);
                let open = panel.clone();
                Tip::new("Start a new, empty chat").child(pressable(b, move |_| open.open_chat("")))
            });
            bar.child(rect().width(Size::flex(1.0)).child(title))
                .maybe_child(new)
                .maybe_child(trash)
        }
    }
}
