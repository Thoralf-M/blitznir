//! The open view's head: what is open, its Copy and Write out, and its
//! Delete, asked once in the head itself ("Delete this dictation? Yes / No");
//! Esc or a press elsewhere takes the question back. And a replacement's
//! prompt, folded to three lines; and the history row a chat is about, its
//! Before and After on the chat's one selection.

use super::{
    answer::Body,
    icons::{self, icon, icon_button},
    section::choice,
    select::Selectable,
    style::{Look, button, head, pressable, use_palette},
    tip::Tip,
};
use crate::panel::{Panel, view::Ui};
use blitznir_core::{
    markdown::{Block, select},
    store::{HistoryItem, HistoryKind},
};
use freya::prelude::*;
use std::sync::Arc;

/// A folded prompt shows this many lines.
const FOLDED: usize = 3;
/// What a chat's head holds beside its title (the rename's two icons,
/// "12 messages", Delete and the gaps, about 175 pt) with some room to
/// spare: the title is cut to leave it them, as a row does not shrink a
/// child, and they were squeezed a letter a line.
const BESIDE_TITLE: f32 = 200.0;

/// What the head's Delete deletes, as it asks and as its tooltip says.
#[derive(PartialEq)]
pub struct Delete {
    pub ask: &'static str,
    pub tip: &'static str,
}

/// The head of the open view: its title, a note (when, how many), Copy and
/// Write out for `text`, and Delete when there is something to delete.
#[derive(PartialEq)]
pub struct Head {
    pub title: String,
    pub note: String,
    pub text: Option<String>,
    pub delete: Option<Delete>,
    /// The title can be renamed and suggested (an open chat's).
    pub rename: bool,
}

impl Component for Head {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let ui = use_consume::<State<Ui>>();
        let asking = ui.read().asking && self.delete.is_some();
        let mut at = use_state(|| None::<Area>);
        let bar = rect()
            .width(Size::fill())
            .height(Size::px(24.0)) // the same with the question: nothing below moves
            .horizontal()
            .content(Content::flex())
            .cross_align(Alignment::Center)
            .spacing(6.0)
            .on_sized(move |e: Event<SizedEventData>| at.set_if_modified(Some(e.area)));
        let outside = panel.clone();
        let bar = bar.on_global_pointer_up(move |e: Event<PointerEventData>| {
            let c = e.global_location();
            let inside = at().is_some_and(|a| a.contains((c.x as f32, c.y as f32).into()));
            if asking && !inside {
                outside.ask_delete(false);
            }
        });
        if let Some(d) = self.delete.as_ref().filter(|_| asking) {
            let (yes, no) = (panel.clone(), panel);
            return bar
                .child(
                    rect()
                        .width(Size::flex(1.0))
                        .child(head(&p, d.ask).font_size(12.0)),
                )
                .child(choice(&p, Look::Plain, "Yes", move |_| yes.delete_viewed()))
                .child(choice(&p, Look::Tool, "No", move |_| no.ask_delete(false)));
        }
        let tools = self.text.clone().map(|text| {
            let (copy, out, text2) = (panel.clone(), panel.clone(), text.clone());
            let tip = "Write out: paste it into the app you came from";
            rect()
                .horizontal()
                .child(icon_button(&p, icons::COPY, "Copy", true, move |_| {
                    copy.copy(text.clone(), "Copied".into());
                }))
                .child(icon_button(&p, icons::OUT, tip, true, move |_| {
                    out.write_out(Some(text2.clone()));
                }))
        });
        let delete = self.delete.as_ref().map(|d| {
            let label = rect()
                .horizontal()
                .cross_align(Alignment::Center)
                .spacing(4.0)
                .child(icon(icons::TRASH, p.muted, 14.0))
                .child(label().text("Delete").font_size(12.0));
            let b = button(&p, Look::Tool).padding((2.0, 6.0)).child(label);
            Tip::new(d.tip).child(pressable(b, move |_| panel.ask_delete(true)))
        });
        let title: Element = if self.rename {
            super::chat_title::ChatTitle {
                name: self.title.clone(),
                id: ui.read().chat_id.clone(),
                max: at().map_or(f32::INFINITY, |a| a.width() - BESIDE_TITLE),
            }
            .into_element()
        } else {
            label()
                .text(self.title.clone())
                .font_weight(FontWeight::SEMI_BOLD)
                .into_element()
        };
        bar.child(title)
            .child(
                label()
                    .text(self.note.clone())
                    .font_size(11.0)
                    .color(p.muted),
            )
            .maybe_child(tools)
            .child(rect().width(Size::flex(1.0)))
            .maybe_child(delete)
    }
}

/// The prompt a replacement came from, `{text}` standing for the Before:
/// muted, monospace, folded to [`FOLDED`] lines with a Show all when longer.
#[derive(PartialEq)]
pub struct Prompt {
    pub action: Option<String>,
    pub text: String,
}

impl Component for Prompt {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let mut all = use_state(|| false);
        let long = self.text.lines().count() > FOLDED || self.text.chars().count() > 240;
        let title = match &self.action {
            Some(a) => format!("Prompt · {a}"),
            None => "Prompt".into(),
        };
        let folded = long && !all();
        let body = Selectable::new()
            .span(self.text.clone())
            .width(Size::fill())
            .in_band()
            .font_family("monospace") // Menlo on macOS: daemon.rs registers it
            .font_size(12.0)
            .color(p.muted)
            .max_lines(folded.then_some(FOLDED));
        // an ellipsis only while folded: Skia draws an ellipsized paragraph
        // without a line limit as one line
        let body = if folded {
            body.text_overflow(TextOverflow::Ellipsis)
        } else {
            body
        };
        let toggle = long.then(|| {
            let says = if all() { "Show less" } else { "Show all" };
            choice(&p, Look::Tool, says, move |_| all.toggle())
        });
        rect()
            .width(Size::fill())
            .spacing(4.0)
            .padding((8.0, 10.0))
            .corner_radius(6.0)
            .background(p.soft)
            .child(head(&p, title).font_size(12.0))
            .child(body)
            .maybe_child(toggle)
    }
}

/// One part of a history row as the card shows it: a heading ("Before",
/// "After") or a text. They are pieces of the chat's selection, so one drag
/// runs from the Before through the After into the chat below.
pub struct Part {
    pub text: String,
    pub heading: bool,
    pub muted: bool,
}

impl Part {
    /// Its pieces' blocks: a heading as bold markdown, a text literal.
    pub fn blocks(&self) -> Vec<Block> {
        select::message(&self.text, self.heading)
    }
}

/// `h`'s parts, in order: a replacement's Before, muted, then its After.
pub fn parts(h: &HistoryItem) -> Vec<Part> {
    let heading = |t: &str| Part {
        text: format!("**{t}**"),
        heading: true,
        muted: true,
    };
    let text = |text, muted| Part {
        text,
        heading: false,
        muted,
    };
    let replaced = h.kind() == HistoryKind::Replacement;
    let mut out = vec![];
    if replaced {
        let original = h.original.as_deref().unwrap_or("(nothing was selected)");
        out.extend([heading("Before"), text(original.to_string(), true)]);
    }
    match h.unanswered() {
        Some(why) => out.extend([heading("After"), text(why, true)]),
        None if replaced => out.extend([heading("After"), text(h.text.clone(), false)]),
        None => out.push(text(h.text.clone(), false)),
    }
    out
}

/// The history row a chat is about, as its first turn: its head (what it
/// is, its stamp, Copy, Write out and, while only it is read, Delete). A
/// replacement shows the prompt it came from and the selection it
/// replaced, muted, before the result. Its parts are pieces of the chat's
/// selection from `base` on.
#[derive(PartialEq)]
pub struct DictationCard {
    pub item: HistoryItem,
    pub deletable: bool,
    pub base: usize,
}

impl Component for DictationCard {
    fn render(&self) -> impl IntoElement {
        let h = &self.item;
        let when = h.ts.replace('T', " ");
        let replaced = h.kind() == HistoryKind::Replacement;
        let delete = self.deletable.then_some(if replaced {
            Delete {
                ask: "Delete this replacement?",
                tip: "Delete this replacement",
            }
        } else {
            Delete {
                ask: "Delete this dictation?",
                tip: "Delete this dictation",
            }
        });
        let top = Head {
            title: h.what().into(),
            note: when.get(..16).unwrap_or(&when).to_string(),
            text: h.unanswered().is_none().then(|| h.text.clone()),
            delete,
            rename: false,
        };
        let prompt = h.prompt.clone().filter(|_| replaced).map(|text| Prompt {
            action: h.action.clone(),
            text,
        });
        // a heading and its text, close together; whether it is the After
        let mut groups: Vec<(Option<bool>, Vec<Element>)> = vec![];
        let mut base = self.base;
        for part in parts(h) {
            let n = select::pieces(&part.blocks()).len();
            if part.heading || groups.is_empty() {
                let side = part.heading.then(|| part.text.contains("After"));
                groups.push((side, vec![]));
            }
            let body = Body {
                text: part.text,
                markdown: part.heading,
                base,
                muted: part.muted,
            };
            base += n;
            if let Some((_, g)) = groups.last_mut() {
                g.push(body.into_element());
            }
        }
        let p = use_palette();
        // Before and After as quotes: a grey bar for what was, the accent
        // for what replaced it
        let groups = groups.into_iter().map(|(side, g)| {
            let bar = |color| {
                let left = BorderWidth {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 3.0,
                };
                Border::new().fill(color).width(left)
            };
            let g = rect().width(Size::fill()).spacing(4.0).children(g);
            match side {
                Some(after) => g.padding((2.0, 0.0, 2.0, 12.0)).border(bar(if after {
                    p.accent
                } else {
                    p.line
                })),
                None => g,
            }
            .into()
        });
        rect()
            .width(Size::fill())
            .spacing(12.0)
            .child(top)
            .maybe_child(prompt)
            .children(groups.collect::<Vec<Element>>())
    }
}
