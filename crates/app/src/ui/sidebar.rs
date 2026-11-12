//! The sidebar: a search, the chats, and History. Chats (a chat icon, the
//! name, how many messages) take the room under the Chats heading, whose
//! New starts one. History, folded at the bottom until asked for (or a
//! search finds something in it), holds Dictations (live, batch,
//! recovered: muted text and its time) and Replacements (rewrites,
//! translations, write-outs: the selection they replaced struck through
//! above the result): there for finding something again. A press on a row
//! opens it, where its Delete is; a heading's trash deletes all of its
//! list, asked once in the heading; a heading's title folds its list away.
//! The history lists never squeeze the chats out of a short window
//! ([`caps`]). state.json keeps whether the sidebar is folded, how wide it
//! was dragged (the splitter in ui/mod.rs), which lists are folded away and
//! whether History is open.

use super::{
    icons::{self, icon, icon_button},
    section::{Section, history_heading},
    style::{Palette, press_keys, ring, selectable_line, use_palette},
    tip::Tip,
};
use crate::panel::{
    List, Panel, list_name,
    view::{self, ChatRow, Ui, short_time},
};
use blitznir_core::store::{HistoryItem, HistoryKind};
use freya::prelude::*;
use room::{clear_of_bar, list};
use std::sync::Arc;

/// A tooltip shows this much of each text at most.
const TIP_CHARS: usize = 300;
/// A press on a row's text that moved further than this was a drag that
/// selected text, not a click that opens the row.
const DRAG_PX: f64 = 4.0;

/// A row's icon button: its icon, tooltip and press.
#[derive(PartialEq)]
struct Tool {
    src: &'static [u8],
    tip: &'static str,
    press: EventHandler<Event<PressEventData>>,
}

/// A list row: an optional icon, its name, a note (when, how many), and in
/// the note's place its icon buttons while hovered or open (Tab, Return
/// opens a row, then Tab reaches them); hover and "open" shade it.
#[derive(PartialEq)]
struct Row {
    lead: Option<&'static [u8]>,
    /// A replacement's original: struck through above the name.
    before: Option<String>,
    name: String,
    /// The name in the text colour (a chat) or muted (a dictation).
    strong: bool,
    note: String,
    on: bool,
    /// The full texts, on hover.
    tip: Option<String>,
    tools: Vec<Tool>,
    press: EventHandler<Event<PressEventData>>,
}

impl Component for Row {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let mut hover = use_state(|| false);
        let tooled = (hover() || self.on) && !self.tools.is_empty();
        let tools = self.tools.iter().filter(|_| tooled).map(|t| {
            let press = t.press.clone();
            icon_button(&p, t.src, t.tip, true, move |e| {
                e.stop_propagation();
                press.call(e);
            })
            .into_element()
        });
        // focusable: Tab reaches a row, Return or Space presses it
        let a11y = use_a11y();
        let keyed = use_focus(a11y)() == Focus::Keyboard;
        let shade = self.on || hover() || keyed;
        let name =
            selectable_line(self.name.clone()).color(if self.strong { p.fg } else { p.muted });
        let before = self.before.clone().map(|b| {
            selectable_line(b)
                .font_size(11.0)
                .color(p.muted)
                .text_decoration(TextDecoration::LineThrough)
        });
        // the text is selectable: a click on it opens the row, a drag that
        // selects does not
        let mut down = use_state(|| None::<CursorPoint>);
        let open = self.press.clone();
        let text = rect()
            .width(Size::fill())
            .padding((3.0, 0.0))
            .on_mouse_down(move |e: Event<MouseEventData>| down.set(Some(e.global_location)))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                let dragged = match (down(), e.data()) {
                    (Some(at), PressEventData::Mouse(m)) => {
                        at.distance_to(m.global_location) > DRAG_PX
                    }
                    _ => false,
                };
                if !dragged {
                    open.call(e);
                }
            })
            .maybe_child(before)
            .child(name);
        let text = match &self.tip {
            Some(tip) => Tip::new(tip.clone())
                .width(Size::flex(1.0))
                .child(text)
                .into_element(),
            None => rect().width(Size::flex(1.0)).child(text).into_element(),
        };
        rect()
            .width(Size::fill())
            .horizontal()
            .content(Content::flex())
            .cross_align(Alignment::Center)
            .spacing(4.0)
            .padding((1.0, 1.0, 1.0, 6.0))
            .corner_radius(5.0)
            .a11y_id(a11y)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::ListItem)
            .background(if shade { p.soft } else { Color::TRANSPARENT })
            .maybe(keyed, |r| r.border(ring(&p)))
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            .on_press(self.press.clone())
            .on_key_down(press_keys(self.press.clone()))
            .maybe_child(self.lead.map(|src| icon(src, p.muted, 13.0)))
            .child(text)
            .maybe_child((!tooled).then(|| {
                label()
                    .text(self.note.clone())
                    .font_size(11.0)
                    .color(p.muted)
            }))
            .children(tools)
    }
}

fn empty(p: &Palette, text: &str) -> Element {
    rect()
        .padding((3.0, 6.0))
        .child(label().text(text.to_string()).color(p.muted))
        .into_element()
}

/// `text` on one line.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// At most [`TIP_CHARS`] of `text`.
fn cut(text: &str) -> String {
    let short: String = text.chars().take(TIP_CHARS).collect();
    let more = if short.len() < text.len() { "…" } else { "" };
    format!("{short}{more}")
}

/// History rows of one kind, with their index in the whole list: a press
/// reads the row in the transcript; the icons copy it and write it out.
fn entries(panel: &Arc<Panel>, ui: &Ui, rows: &[(usize, &HistoryItem)]) -> Vec<Element> {
    let reading = ui.about.as_ref().filter(|_| view::viewing(ui));
    let row = |(i, r): &(usize, &HistoryItem)| {
        let unanswered = r.unanswered();
        let words = unanswered.clone().unwrap_or_else(|| one_line(&r.text));
        let cut60: String = words.chars().take(60).collect();
        let more = if cut60.len() < words.len() { "…" } else { "" };
        let said = format!("Copied: {cut60}{more}");
        let (open, copy, out) = (panel.clone(), panel.clone(), panel.clone());
        let (text, text2, i) = (r.text.clone(), r.text.clone(), *i);
        let replaced = r.kind() == HistoryKind::Replacement;
        let tip = replaced.then(|| {
            let before = r.original.as_deref().unwrap_or("(nothing was selected)");
            format!("Before: {}\nAfter: {}", cut(before), cut(&words))
        });
        let copy = Tool {
            src: icons::COPY,
            tip: "Copy",
            press: EventHandler::new(move |_| copy.copy(text.clone(), said.clone())),
        };
        let out = Tool {
            src: icons::OUT,
            tip: "Write out: paste it into the app you came from",
            press: EventHandler::new(move |_| out.write_out(Some(text2.clone()))),
        };
        Row {
            lead: None,
            before: r.original.as_deref().map(one_line),
            name: words,
            strong: replaced,
            note: short_time(&r.ts),
            on: reading.is_some_and(|h| h == *r),
            tip,
            // a cancelled row has no text to copy: its prompt is in the transcript
            tools: if unanswered.is_some() {
                vec![]
            } else {
                vec![copy, out]
            },
            press: EventHandler::new(move |_| open.open_recent(i)),
        }
        .into_element()
    };
    rows.iter().map(row).collect()
}

fn chats(panel: &Arc<Panel>, ui: &Ui, rows: &[&ChatRow]) -> Vec<Element> {
    let row = |r: &&ChatRow| {
        let (open, id) = (panel.clone(), r.id.clone());
        Row {
            lead: Some(icons::CHAT),
            before: None,
            name: if r.name.is_empty() {
                "Chat".into()
            } else {
                r.name.clone()
            },
            strong: true,
            note: r.messages.to_string(),
            on: ui.chatting && r.id == ui.chat_id && !view::viewing(ui),
            tip: None,
            tools: vec![],
            press: EventHandler::new(move |_| open.open_chat(&id)),
        }
        .into_element()
    };
    rows.iter().map(row).collect()
}

/// The search field: filters as it is typed; Esc empties it first; ⌘F
/// focuses it (the id is Settings' search's: never both on screen).
fn search(p: &Palette, mut query: State<String>) -> Input {
    let nav = use_consume::<super::form_nav::FormNav>();
    super::form_nav::use_find_focus(&nav);
    let keys = move |e: Event<KeyboardEventData>| {
        super::keys::field_keys(&e, |e| {
            let esc = matches!(e.key, Key::Named(NamedKey::Escape));
            (esc && !query.peek().is_empty()).then(|| {
                query.set(String::new());
                false
            })
        })
    };
    Input::new(query)
        .a11y_id(nav.search_id)
        .placeholder("Search chats and history")
        .width(Size::fill())
        .compact()
        .leading(icon(icons::SEARCH, p.muted, 13.0))
        .background(p.field)
        .focus_background(p.field)
        .border_fill(p.line)
        .focus_border_fill(p.accent)
        .color(p.fg)
        .placeholder_color(p.muted)
        .on_pre_key_down(keys)
}

/// The rows, or why there are none.
fn or_none(p: &Palette, rows: Vec<Element>, none: &str) -> Vec<Element> {
    if rows.is_empty() {
        vec![empty(p, none)]
    } else {
        rows
    }
}

/// The history rows of `kind` that hold `q` (lowercase; empty: all), with
/// their index in the history, and how many of `kind` there are.
fn hits<'a>(ui: &'a Ui, q: &str, kind: HistoryKind) -> (Vec<(usize, &'a HistoryItem)>, usize) {
    // the prompt too: a Cancelled row has no text to find it by
    let hit = |r: &HistoryItem| {
        let has = |s: Option<&str>| s.unwrap_or_default().to_lowercase().contains(q);
        has(Some(&r.text)) || has(r.original.as_deref()) || has(r.prompt.as_deref())
    };
    let all = (ui.recent.iter().enumerate()).filter(|(_, r)| r.kind() == kind);
    let total = all.clone().count();
    (all.filter(|(_, r)| hit(r)).collect(), total)
}

#[derive(PartialEq)]
pub struct Sidebar {
    /// In points: state.json's, or the splitter's while it is dragged.
    pub width: f32,
}

impl Component for Sidebar {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let ui = use_consume::<State<Ui>>();
        let ui = ui.read();
        let query = use_state(String::new);
        let q = query.read().trim().to_lowercase();
        let found = !q.is_empty();
        let (said, said_all) = hits(&ui, &q, HistoryKind::Dictation);
        let (replaced, replaced_all) = hits(&ui, &q, HistoryKind::Replacement);
        let talks: Vec<_> = ui
            .chats
            .iter()
            .filter(|c| c.haystack.contains(&q))
            .collect();
        let none = |text| if found { "No matches" } else { text };
        let rows = |found: &[(usize, &HistoryItem)], text| {
            or_none(&p, entries(&panel, &ui, found), none(text))
        };
        let said_rows = rows(&said, "Dictations show up here.");
        let replaced_rows = rows(
            &replaced,
            "Rewrites, translations and write-outs show up here.",
        );
        let chat_rows = or_none(&p, chats(&panel, &ui, &talks), none("No saved chats yet."));
        // a search opens every list: no match hides in a folded one
        let closed = |l: List| !found && ui.closed.iter().any(|c| c == list_name(l));
        let (dict, repl) = (
            List::Rows(HistoryKind::Dictation),
            List::Rows(HistoryKind::Replacement),
        );
        // History is for finding something again: folded unless asked for,
        // or a search finds something in it
        let in_history = !said.is_empty() || !replaced.is_empty();
        let history = ui.history_open || (found && in_history);
        let chats_open = !closed(List::Chats);
        let shows = [dict, repl].map(|l| history && !closed(l));
        let (cap, laid, edge) = room::use_room(shows, chats_open, ui.history_share);
        let section = |kind, title, shown, total| Section {
            kind,
            title,
            shown,
            total,
            closed: closed(kind),
            held: found,
        };
        let lists = history.then(|| {
            rect()
                .width(Size::fill())
                .padding((0.0, 0.0, 0.0, 10.0))
                .child(section(dict, "Dictations", said.len(), said_all))
                .maybe_child((!closed(dict)).then(|| list(said_rows, cap, (laid.clone(), 0))))
                .child(section(repl, "Replacements", replaced.len(), replaced_all))
                .maybe_child((!closed(repl)).then(|| list(replaced_rows, cap, (laid.clone(), 1))))
        });
        // the chats take the room; History waits at the bottom
        let chats_list = ScrollView::new()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .child(clear_of_bar(
                chat_rows.into_iter().filter(|_| chats_open).collect(),
            ));
        rect()
            .width(Size::px(self.width))
            .height(Size::fill())
            .background(p.side)
            .padding((8.0, 6.0, 6.0, 6.0))
            .content(Content::flex())
            .child(search(&p, query))
            .child(section(List::Chats, "Chats", talks.len(), ui.chats.len()))
            .child(chats_list)
            .child(edge)
            .child(history_heading(
                &p,
                &panel,
                (history, found && in_history),
                said_all + replaced_all,
            ))
            .maybe_child(lists)
    }
}

mod room;
#[cfg(test)]
mod tests;
