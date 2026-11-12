//! Settings as a form, a page per section (ui/form_page.rs): your buttons
//! as a table, your servers, then every other key of config.toml. Each
//! input changes the editor's text (panel/form.rs), so the Text tab shows
//! the same file; it saves itself (panel/form.rs).

use super::{
    form_keys::KeysInput,
    form_nav::FormNav,
    form_page::{group, header, row, section},
    icons::{self, icon, icon_button},
    keys::field_keys,
    select::Selectable,
    style::{Look, Palette, button, pressable, use_palette},
};
use crate::panel::{Panel, form::Target, view::Ui};
use blitznir_core::config::form::{self, Kind, Rows, SECTIONS, cards};
use freya::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

/// A field's keys: its own, except Esc, Tab, ⌘S and ⌘, (the window's).
fn keys(e: Event<KeyboardEventData>) -> bool {
    field_keys(&e, |_| None)
}

fn choice(
    p: &Palette,
    on: bool,
    text: &str,
    press: impl FnMut(Event<PressEventData>) + 'static,
) -> Rect {
    let look = if on { Look::Default } else { Look::Plain };
    pressable(
        button(p, look)
            .padding((3.0, 10.0))
            .child(label().text(text.to_string())),
        press,
    )
}

/// One input of the form. What is typed stays here while the file refuses
/// it (the reason below); a change of the file from elsewhere comes in.
#[derive(PartialEq)]
pub(super) struct Edit {
    target: Target,
    value: String,
    error: Option<String>,
    /// A prompt opened to edit: room for it.
    tall: bool,
}

/// A volume from 0 to 100 as a slider in steps of 5, its value beside it.
fn volume(p: &Palette, panel: &Arc<Panel>, target: Target, value: &str) -> Element {
    let set = panel.clone();
    let now = value.trim().parse::<f64>().unwrap_or(0.0).clamp(0.0, 100.0);
    let moved = move |v: f64| set.form_set(target, format!("{}", v.round() as u8));
    let shown = if now == 0.0 {
        "Off".to_string()
    } else {
        format!("{now:.0} %")
    };
    let slider = Slider::new(moved)
        .value(now)
        .step(5.0)
        .size(Size::px(220.0));
    rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(10.0)
        .child(slider)
        .child(label().text(shown).color(p.muted))
        .into()
}

/// What an empty input shows: its field's example.
fn example(target: Target) -> &'static str {
    match target {
        Target::Field(f) => f.example,
        Target::Cell(..) => "",
    }
}

impl Component for Edit {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let mut local = use_state(|| self.value.clone());
        let seen = use_hook(|| Rc::new(RefCell::new(self.value.clone())));
        if *seen.borrow() != self.value {
            *seen.borrow_mut() = self.value.clone();
            local.set_if_modified(self.value.clone());
        }
        let target = self.target;
        // the effect keeps its first closure: it reads the input's target
        // now, as one Edit serves every button's prompt in turn
        let aim = use_hook(|| Rc::new(Cell::new(target)));
        aim.set(target);
        let (push, was, at) = (panel.clone(), seen.clone(), aim.clone());
        use_side_effect(move || {
            let now = local.read().clone();
            if now != *was.borrow() {
                push.form_set(at.get(), now);
            }
        });
        let field = |height: Option<f32>| {
            let input = Input::new(local)
                .placeholder(example(target))
                .placeholder_color(p.muted)
                .width(Size::fill())
                .background(p.field)
                .focus_background(p.field)
                .border_fill(if self.error.is_some() {
                    p.critical
                } else {
                    p.line
                })
                .focus_border_fill(p.accent)
                .color(p.fg)
                .on_pre_key_down(keys);
            match height {
                Some(h) => input.multiline(true).height(Size::px(h)).into_element(),
                None => input.into_element(),
            }
        };
        let input: Element = match target.kind() {
            Kind::Bool => {
                let on = self.value == "true";
                let flip = panel.clone();
                let set = move |v: bool| flip.form_set(target, v.to_string());
                slim(Switch::new(WritableSetter::new(on, set))).into()
            }
            Kind::Choice(options) => rect()
                .horizontal()
                .spacing(4.0)
                .children(options.iter().map(|o| {
                    let pick = panel.clone();
                    choice(&p, self.value == *o, o, move |_| {
                        pick.form_set(target, (*o).into())
                    })
                    .into_element()
                }))
                .into(),
            Kind::Percent => volume(&p, &panel, target, &self.value),
            Kind::Keys => KeysInput {
                local,
                target,
                error: self.error.is_some(),
            }
            .into_element(),
            Kind::Long => field(Some(if self.tall { 320.0 } else { 140.0 })),
            Kind::Map => field(Some(72.0)),
            _ => field(None),
        };
        let error = self
            .error
            .clone()
            .map(|e| label().text(e).font_size(12.0).color(p.critical));
        // a switch, a choice or a slider is as wide as itself
        let own = matches!(target.kind(), Kind::Bool | Kind::Choice(_) | Kind::Percent);
        rect()
            .width(if own { Size::auto() } else { Size::fill() })
            .spacing(3.0)
            .child(input)
            .maybe_child(error)
    }
}

pub(super) fn edit(ui: &Ui, target: Target, value: String) -> Edit {
    let id = target.id();
    let error = (ui.form_error.as_ref()).and_then(|(at, e)| (*at == id).then(|| e.clone()));
    Edit {
        target,
        value,
        error,
        tall: false,
    }
}

impl Edit {
    pub(super) fn tall(self) -> Edit {
        Edit { tall: true, ..self }
    }
}

/// A label and its help, both selectable (the button editor's).
pub(super) fn title(p: &Palette, text: &str, help: &str) -> Rect {
    let name = Selectable::new()
        .span(text.to_string())
        .width(Size::fill())
        .font_size(13.0)
        .font_weight(FontWeight::SEMI_BOLD);
    let help = (!help.is_empty()).then(|| {
        Selectable::new()
            .span(help.to_string())
            .width(Size::fill())
            .font_size(12.0)
            .color(p.muted)
    });
    rect()
        .width(Size::fill())
        .spacing(1.0)
        .child(name)
        .maybe_child(help)
}

pub(super) fn card(p: &Palette) -> Rect {
    rect()
        .width(Size::fill())
        .spacing(8.0)
        .padding((10.0, 12.0))
        .corner_radius(6.0)
        .background(p.soft)
}

pub(super) fn add(p: &Palette, panel: &Arc<Panel>, rows: Rows, text: &str) -> Rect {
    let panel = panel.clone();
    let inner = rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(6.0)
        .child(icon(icons::PLUS, p.muted, 14.0))
        .child(label().text(text.to_string()));
    pressable(
        button(p, Look::Tool).padding((4.0, 8.0)).child(inner),
        move |_| panel.form_add(rows),
    )
}

/// Error about a whole table (a row that could not be added or removed).
pub(super) fn table_error(p: &Palette, ui: &Ui, rows: Rows) -> Option<Label> {
    let id = format!("{rows:?}");
    let e = ui.form_error.as_ref().filter(|(at, _)| *at == id)?;
    Some(label().text(e.1.clone()).font_size(12.0).color(p.critical))
}

/// The OpenAI-compatible servers, a group each: its name and Remove, then
/// a row per column.
fn servers(p: &Palette, panel: &Arc<Panel>, ui: &Ui, text: &str) -> Rect {
    let rows = form::rows(text, Rows::Endpoints);
    let server = |(i, cells): (usize, &Vec<String>)| {
        let gone = panel.clone();
        let del = icon_button(p, icons::TRASH, "Remove this server", true, move |_| {
            gone.form_remove(Rows::Endpoints, i);
        });
        let name = rect()
            .width(Size::fill())
            .horizontal()
            .content(Content::flex())
            .cross_align(Alignment::Center)
            .padding((8.0, 10.0, 8.0, 14.0))
            .child(
                label()
                    .text(cells[0].clone())
                    .font_weight(FontWeight::SEMI_BOLD)
                    .width(Size::flex(1.0)),
            )
            .child(del)
            .into_element();
        let fields = form::ENDPOINTS.iter().enumerate().map(|(c, col)| {
            let control = edit(ui, Target::Cell(Rows::Endpoints, i, col), cells[c].clone());
            row(p, col.label, "", col.kind, control)
                .key(c)
                .into_element()
        });
        group(p, std::iter::once(name).chain(fields).collect())
            .key(i)
            .into_element()
    };
    rect()
        .width(Size::fill())
        .spacing(12.0)
        .children(rows.iter().enumerate().map(server))
        .maybe_child(table_error(p, ui, Rows::Endpoints))
        .child(add(p, panel, Rows::Endpoints, "Add server"))
}

#[derive(PartialEq)]
pub struct Form;

impl Component for Form {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let ui = use_consume::<State<Ui>>();
        let ui = ui.read();
        let nav = use_consume::<FormNav>();
        let text = ui.config.text.text.clone();
        // a search counts while its field shows: not from a folded sidebar
        let super::Peek(peek) = use_consume::<super::Peek>();
        let side = if super::window_size().width < super::NARROW {
            peek()
        } else {
            !ui.folded
        };
        let query = if side {
            nav.query.read().clone()
        } else {
            String::new()
        };
        let searching = !query.trim().is_empty();
        let shown = cards(&query);
        // the picked page, or what the search finds on every page
        let pages: Vec<usize> = if searching {
            (0..shown.len()).filter(|&i| shown[i]).collect()
        } else {
            vec![*nav.page.read()]
        };
        let page = |i: usize| -> Element {
            let (title, about) = super::form_nav::page_title(i);
            let about = if searching { "" } else { about };
            let body: Element = match i {
                0 => rect()
                    .width(Size::fill())
                    .spacing(12.0)
                    .child(header(&p, title, about))
                    .child(super::form_buttons::Buttons { text: text.clone() })
                    .into(),
                1 => rect()
                    .width(Size::fill())
                    .spacing(12.0)
                    .child(header(&p, title, about))
                    .child(servers(&p, &panel, &ui, &text))
                    .into(),
                _ => section(&p, &ui, &text, &SECTIONS[i - 2], &query, nav.advanced).into(),
            };
            rect().key(i).width(Size::fill()).child(body).into()
        };
        let none = (searching && pages.is_empty()).then(|| {
            label()
                .text(format!("No setting matches \"{}\"", query.trim()))
                .color(p.muted)
        });
        ScrollView::new_controlled(nav.scroll)
            .width(Size::fill())
            .height(Size::flex(1.0))
            .child(
                rect()
                    .width(Size::fill())
                    .spacing(28.0)
                    .padding((8.0, 4.0, 16.0, 0.0))
                    .maybe_child(none)
                    .children(pages.into_iter().map(page)),
            )
    }
}

/// A switch the height of a line of the form, not Freya's 48×28 one. The
/// thumb's travel is what the track leaves inside its padding: 34 − 2·3 =
/// 12 (toggled thumb) + 16 (its offset).
fn slim(s: Switch) -> Switch {
    s.width(34.)
        .height(18.)
        .padding(3.)
        .thumb_size(10.)
        .toggled_thumb_size(12.)
        .pressed_thumb_size_offset(2.)
        .thumb_offset(1.)
        .toggled_thumb_offset(16.)
}
