//! The Settings form's pages, one per section (the sidebar picks it): a
//! title and what the section is for, then its settings as rows on one
//! ground, the label and help on the left and the control on the right, or
//! under them for a text with lines; the rarely changed ones folded under
//! Advanced. A search shows what matches from every page instead.

use super::form::{Edit, edit};
use super::icons::{self, icon};
use super::select::Selectable;
use super::style::{Look, Palette, button, pressable};
use crate::panel::form::Target;
use crate::panel::view::Ui;
use blitznir_core::config::form::{self, Field, Kind, Section};
use freya::prelude::*;

/// A page's title and, unless searching, what it is for.
pub(super) fn header(p: &Palette, title: &str, about: &str) -> Rect {
    let about = (!about.is_empty()).then(|| {
        Selectable::new()
            .span(about.to_string())
            .width(Size::fill())
            .font_size(12.5)
            .color(p.muted)
    });
    rect()
        .width(Size::fill())
        .spacing(4.0)
        .padding((2.0, 2.0, 4.0, 2.0))
        .child(
            label()
                .text(title.to_string())
                .font_size(20.0)
                .font_weight(FontWeight::BOLD),
        )
        .maybe_child(about)
}

/// Rows on one rounded ground, a hairline between two.
pub(super) fn group(p: &Palette, rows: Vec<Element>) -> Rect {
    let line = p.line;
    let rule = move || {
        rect()
            .width(Size::fill())
            .padding((0.0, 0.0, 0.0, 14.0))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(1.0))
                    .background(line),
            )
            .into_element()
    };
    let parted = rows
        .into_iter()
        .enumerate()
        .flat_map(move |(i, r)| (i > 0).then(rule).into_iter().chain([r]));
    rect()
        .width(Size::fill())
        .corner_radius(8.0)
        .background(p.soft)
        .children(parted)
}

/// Where a kind's control goes: beside the label at this width (None: its
/// own), or under it, full width.
fn beside(kind: Kind) -> Option<Option<f32>> {
    match kind {
        Kind::Long | Kind::Map | Kind::List => None,
        Kind::Keys => Some(Some(260.0)),
        Kind::Text => Some(Some(260.0)),
        Kind::Int | Kind::Float => Some(Some(110.0)),
        _ => Some(None),
    }
}

/// One setting: its label and help, and its control.
pub(super) fn row(p: &Palette, name: &str, help: &str, kind: Kind, control: Edit) -> Rect {
    let text = |width: Size| {
        let help = (!help.is_empty()).then(|| {
            Selectable::new()
                .span(help.to_string())
                .width(Size::fill())
                .font_size(11.5)
                .color(p.muted)
        });
        rect()
            .width(width)
            .spacing(2.0)
            .child(
                Selectable::new()
                    .span(name.to_string())
                    .width(Size::fill())
                    .font_size(13.0),
            )
            .maybe_child(help)
    };
    let row = rect().width(Size::fill()).padding((10.0, 14.0));
    match beside(kind) {
        None => row.spacing(8.0).child(text(Size::fill())).child(control),
        Some(width) => row
            .horizontal()
            .content(Content::flex())
            .cross_align(Alignment::Center)
            .spacing(24.0)
            .child(text(Size::flex(1.0)))
            .child(
                rect()
                    .width(width.map_or(Size::auto(), Size::px))
                    .child(control),
            ),
    }
}

fn field_row(p: &Palette, ui: &Ui, text: &str, f: &'static Field) -> Element {
    let control = edit(ui, Target::Field(f), form::value(text, f));
    row(p, f.label, f.help, f.kind, control)
        .key(f.key)
        .into_element()
}

/// Section `s`: while searching, what matches `query` in one group; else
/// its settings, and the advanced ones folded unless `advanced` is open.
pub(super) fn section(
    p: &Palette,
    ui: &Ui,
    text: &str,
    s: &'static Section,
    query: &str,
    mut advanced: State<bool>,
) -> Rect {
    let searching = !query.trim().is_empty();
    let shown = s
        .fields
        .iter()
        .filter(|f| form::field_matches(f, s.title, query));
    let (rare, common): (Vec<&'static Field>, Vec<&'static Field>) =
        shown.partition(|f| f.advanced && !searching);
    let rows = |fields: &[&'static Field]| -> Vec<Element> {
        fields.iter().map(|f| field_row(p, ui, text, f)).collect()
    };
    let open = advanced();
    // folded as the sidebar's lists fold: a chevron, the count when shut
    let fold = (!rare.is_empty()).then(|| {
        let what = if open {
            "Advanced".to_string()
        } else {
            format!("Advanced ({})", rare.len())
        };
        let inner = rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(4.0)
            .child(icon(
                if open { icons::OPEN } else { icons::SHUT },
                p.muted,
                14.0,
            ))
            .child(
                label()
                    .text(what)
                    .color(p.muted)
                    .font_weight(FontWeight::SEMI_BOLD),
            );
        pressable(
            button(p, Look::Tool).padding((4.0, 6.0)).child(inner),
            move |_| advanced.set(!open),
        )
    });
    rect()
        .width(Size::fill())
        .spacing(12.0)
        .child(header(p, s.title, if searching { "" } else { s.about }))
        .maybe_child((!common.is_empty()).then(|| group(p, rows(&common))))
        .maybe_child(fold)
        .maybe_child((open && !rare.is_empty()).then(|| group(p, rows(&rare))))
}
