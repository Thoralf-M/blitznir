//! The Settings form's Buttons table, one line per button: its label, its
//! prompt as a one-line preview (a press opens it large to edit), which one
//! Return runs outside and inside a chat, remove. The last line, Chat /
//! Send, is what Return does where no button is marked.

use super::{
    form::{add, card, edit, table_error, title},
    icons::{self, icon_button},
    style::{Look, Palette, button, pressable, use_palette},
    tip::Tip,
};
use crate::panel::{
    Panel,
    form::Target,
    view::{RET, Ui},
};
use blitznir_core::config::form::{self, Column, Rows};
use freya::prelude::*;
use std::sync::Arc;

const LABEL_W: f32 = 110.0;
/// The label column's least, in a narrow window.
const LABEL_MIN: f32 = 64.0;
const PICK_W: f32 = 76.0;
const TRASH_W: f32 = 26.0;

/// The label column for a table `table` wide: its own width while there is
/// room, else half of what the fixed columns leave, the prompt's preview
/// the other half, so the trash column stays in a 420 pt window.
fn label_width(table: f32) -> f32 {
    let room = table - 2.0 * PICK_W - (TRASH_W + 4.0) - 2.0;
    (room / 2.0).clamp(LABEL_MIN, LABEL_W + 12.0)
}

/// A radio of an "Action on Return" column: a ring, filled where it is on.
fn radio(
    p: &Palette,
    on: bool,
    tip: String,
    press: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let dot = rect()
        .width(Size::px(14.0))
        .height(Size::px(14.0))
        .corner_radius(7.0)
        .border(
            Border::new()
                .fill(if on { p.accent } else { p.muted })
                .width(1.5),
        )
        .main_align(Alignment::Center)
        .cross_align(Alignment::Center)
        .maybe_child(on.then(|| {
            rect()
                .width(Size::px(7.0))
                .height(Size::px(7.0))
                .corner_radius(3.5)
                .background(p.accent)
        }));
    let b = pressable(button(p, Look::Tool).padding(4.0).child(dot), press);
    rect()
        .width(Size::px(PICK_W))
        .horizontal()
        .main_align(Alignment::Center)
        .child(Tip::new(tip).child(b))
        .into_element()
}

/// A cell of the table: `h` tall, its content centred up and down,
/// a rule on its right unless it is the last.
fn cell(p: &Palette, width: Size, h: f32, pad: f32, last: bool) -> Rect {
    let right = if last { 0.0 } else { 1.0 };
    let rule = BorderWidth {
        right,
        ..BorderWidth::default()
    };
    rect()
        .width(width)
        .height(Size::px(h))
        .padding((4.0, pad))
        .main_align(Alignment::Center)
        .border(Border::new().fill(p.line).width(rule))
}

/// One line of the table, `h` tall: label (`lw` wide), prompt, the two
/// picks, remove; a rule under it unless it is the last.
fn line(p: &Palette, (h, lw): (f32, f32), last: bool, cells: [Option<Element>; 5]) -> Rect {
    let [label, prompt, a, b, end] = cells;
    let under = BorderWidth {
        bottom: if last { 0.0 } else { 1.0 },
        ..BorderWidth::default()
    };
    rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::flex())
        .border(Border::new().fill(p.line).width(under))
        .child(cell(p, Size::px(lw), h, 6.0, false).maybe_child(label))
        .child(cell(p, Size::flex(1.0), h, 6.0, false).maybe_child(prompt))
        .child(cell(p, Size::px(PICK_W), h, 0.0, false).maybe_child(a))
        .child(cell(p, Size::px(PICK_W), h, 0.0, false).maybe_child(b))
        .child(cell(p, Size::px(TRASH_W + 4.0), h, 2.0, true).maybe_child(end))
}

/// Row heights: up to three lines of 12 pt text (the head, Chat / Send), a field.
const HEAD_H: f32 = 52.0;
const ROW_H: f32 = 40.0;

fn small(p: &Palette, text: impl Into<String>) -> Label {
    label().text(text.into()).font_size(12.0).color(p.muted)
}

fn column_head(p: &Palette, text: String, tip: &str) -> Element {
    rect()
        .width(Size::px(PICK_W))
        .horizontal()
        .main_align(Alignment::Center)
        .child(
            Tip::new(tip).child(
                small(p, text)
                    .width(Size::px(PICK_W - 8.0))
                    .text_align(TextAlign::Center),
            ),
        )
        .into_element()
}

fn head(p: &Palette, lw: f32) -> Rect {
    let picks = [
        column_head(
            p,
            format!("Action on {RET}"),
            "The button that pressing Return runs, outside a chat",
        ),
        column_head(
            p,
            format!("Action on {RET} in chat"),
            "The button that pressing Return runs while you chat",
        ),
    ];
    let [a, b] = picks;
    line(
        p,
        (HEAD_H, lw),
        false,
        [
            Some(small(p, "Label").into()),
            Some(small(p, "Prompt").into()),
            Some(a),
            Some(b),
            None,
        ],
    )
}

/// The last line, Chat / Send: what Return does where no button is marked.
fn chat_line(p: &Palette, lw: f32, enter: Element, chat_enter: Element) -> Rect {
    // in its three lines' height, cut short in a narrow window
    let what = small(p, "Start a chat, or send the message in one")
        .max_lines(3)
        .text_overflow(TextOverflow::Ellipsis);
    line(
        p,
        (HEAD_H, lw),
        true,
        [
            Some(small(p, "Chat / Send").into()),
            Some(what.into()),
            Some(enter),
            Some(chat_enter),
            None,
        ],
    )
}

/// The prompt as one line; a press opens it large.
fn preview(
    p: &Palette,
    prompt: &str,
    open: bool,
    press: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    let first = prompt.trim().lines().next().unwrap_or_default().to_string();
    let text = label()
        .text(first)
        .font_size(12.0)
        .color(if open { p.fg } else { p.muted })
        .max_lines(1)
        .text_overflow(TextOverflow::Ellipsis);
    let b = button(p, if open { Look::Default } else { Look::Plain })
        .width(Size::fill())
        .padding((5.0, 8.0))
        .child(rect().width(Size::fill()).child(text)); // Button centres
    let tip = if open {
        "Close the prompt"
    } else {
        "Open the prompt to edit it"
    };
    Tip::new(tip).child(pressable(b, press)).into_element()
}

/// Why a Return pick was refused.
fn pick_error(p: &Palette, ui: &Ui) -> Option<Label> {
    let id = format!("{:?}.", Rows::Actions);
    let picked = |at: &str| at.ends_with(".enter") || at.ends_with(".chat_enter");
    (ui.form_error.as_ref())
        .filter(|(at, _)| at.starts_with(&id) && picked(at))
        .map(|(_, e)| label().text(e.clone()).font_size(12.0).color(p.critical))
}

/// Button `i`'s prompt, large, under the table, and its hotkey.
fn editor(
    p: &Palette,
    ui: &Ui,
    i: usize,
    cells: &[String],
    (prompt, hotkey): (&'static Column, &'static Column),
    mut open: State<Option<usize>>,
) -> Rect {
    let done = pressable(
        button(p, Look::Plain).child(label().text("Done")),
        move |_| open.set(None),
    );
    rect()
        .width(Size::fill())
        .spacing(4.0)
        .padding((8.0, 0.0, 0.0, 0.0))
        .child(title(
            p,
            &format!("Prompt · {}", cells[0]),
            "{text} is the selection, {instruction} what you typed (empty when you typed nothing)",
        ))
        .child(edit(ui, Target::Cell(Rows::Actions, i, prompt), cells[1].clone()).tall())
        .child(title(
            p,
            "Hotkey",
            "Runs this button on the selection with no panel, as the quick action does: a double-tapped modifier or an F-key, e.g. F14; empty = none",
        ))
        .child(edit(ui, Target::Cell(Rows::Actions, i, hotkey), cells[4].clone()))
        .child(done)
}

#[derive(PartialEq)]
pub struct Buttons {
    pub text: String,
}

impl Component for Buttons {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let ui = use_consume::<State<Ui>>();
        let ui = ui.read();
        let mut open = use_state(|| None::<usize>);
        // the table's width, measured: wide until it is
        let mut wide = use_state(|| f32::INFINITY);
        let lw = label_width(wide());
        let rows = form::rows(&self.text, Rows::Actions);
        let [label_col, prompt_col, enter_col, chat_col, key_col] = form::ACTIONS else {
            unreachable!("the five columns of schema.rs")
        };
        let marked = |c: usize| rows.iter().position(|r| r[c] == "true");
        let (enter, chat_enter) = (marked(2), marked(3));
        let pick = |col: &'static Column, i: Option<usize>, tip: String| {
            let panel = panel.clone();
            let on = if col.key == "enter" {
                enter == i
            } else {
                chat_enter == i
            };
            radio(&p, on, tip, move |_| panel.form_pick(col, i))
        };
        let row = |(i, cells): (usize, &Vec<String>)| {
            let gone = panel.clone();
            let del = icon_button(&p, icons::TRASH, "Remove this button", true, move |_| {
                // the open prompt stays with its button as the rows move up
                if gone.form_remove(Rows::Actions, i) {
                    open.set(open().filter(|o| *o != i).map(|o| o - usize::from(o > i)));
                }
            });
            let name = edit(
                &ui,
                Target::Cell(Rows::Actions, i, label_col),
                cells[0].clone(),
            );
            let shown = open() == Some(i);
            let prompt = preview(&p, &cells[1], shown, move |_| {
                open.set((!shown).then_some(i))
            });
            let a = pick(enter_col, Some(i), format!("{RET} runs {}", cells[0]));
            let b = pick(
                chat_col,
                Some(i),
                format!("{RET} in a chat runs {}", cells[0]),
            );
            let cells = [
                Some(name.into_element()),
                Some(prompt),
                Some(a),
                Some(b),
                Some(del.into_element()),
            ];
            line(&p, (ROW_H, lw), false, cells).key(i).into_element()
        };
        let chat = chat_line(
            &p,
            lw,
            pick(enter_col, None, format!("{RET} starts a chat")),
            pick(chat_col, None, format!("{RET} in a chat sends the message")),
        );
        let table = rect()
            .width(Size::fill())
            .corner_radius(6.0)
            .overflow(Overflow::Clip)
            .border(Border::new().fill(p.line).width(1.0))
            .on_sized(move |e: Event<SizedEventData>| wide.set_if_modified(e.area.width()))
            .child(head(&p, lw))
            .children(rows.iter().enumerate().map(row))
            .child(chat);
        let editing = open().filter(|i| *i < rows.len());
        card(&p)
            .child(table)
            .maybe_child(pick_error(&p, &ui))
            .maybe_child(table_error(&p, &ui, Rows::Actions))
            .maybe_child(editing.map(|i| editor(&p, &ui, i, &rows[i], (prompt_col, key_col), open)))
            .child(add(&p, &panel, Rows::Actions, "Add button"))
    }
}
