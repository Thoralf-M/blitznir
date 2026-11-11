//! The box by the text (panel/revert.rs): the dictation as it was and
//! Revert, or the rewrite and Use rewrite, with the switch between the two
//! for next time; or a translation of text no app can take back, with Copy
//! and Open in Blitznir (its chat in the panel); or, with `rewrite_auto`
//! off, the dictation as typed with Rewrite. A window of its own over the
//! text, shown without taking the key focus, so typing goes on in the app and
//! ends the offer; clicked, its text can be selected and copied, and Esc
//! closes it.

use super::icons::{self, icon_button};
use super::section::choice;
use super::select::Selectable;
use super::style::{self, Look, Palette, button, head, pressable};
use crate::panel::Panel;
use blitznir_core::platform::Rect;
use freya::prelude::*;
use freya::winit::window::WindowLevel;
use std::sync::Arc;

/// Which box it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxKind {
    /// The rewrite replaced the dictation: the dictation, with Revert.
    Revert,
    /// The dictation stayed: the rewrite, with Use rewrite.
    Suggest,
    /// A translation of text no app can take back: Copy, Open in Blitznir.
    Translation,
    /// The dictation as typed (`rewrite_auto = false`): Rewrite runs it.
    Later,
}

/// What the box shows.
#[derive(Clone, PartialEq)]
pub struct Shown {
    /// The text it offers: the dictation, the rewrite or the translation.
    pub text: String,
    pub kind: BoxKind,
    /// `[dictation] rewrite_replaces` as the box opens, for its Replace /
    /// Suggest switch; None: no switch (a translation, or a rewrite asked
    /// for by a click, which always replaces).
    pub replaces: Option<bool>,
}

pub fn config(panel: Arc<Panel>, shown: Shown, at: Rect) -> WindowConfig {
    WindowConfig::new(move || app(panel.clone(), shown.clone()))
        .with_title("Blitznir revert")
        .with_app_id(crate::APP_ID)
        .with_size(at.w, at.h)
        .with_decorations(false)
        .with_resizable(false)
        .with_window_attributes(move |attrs, _| {
            let attrs = attrs
                .with_window_level(WindowLevel::AlwaysOnTop)
                .with_active(false);
            #[cfg(target_os = "macos")]
            let attrs = attrs.with_position(freya::winit::dpi::LogicalPosition::new(at.x, at.y));
            attrs
        })
}

/// The box's content (pub for the UI tests, which render it headless).
pub fn app(panel: Arc<Panel>, shown: Shown) -> impl IntoElement {
    let theme = use_init_theme(|| style::theme(super::dark()));
    let p = Palette::of(&theme.read());
    let replaces = use_state(|| shown.replaces.unwrap_or(true));
    let keys = panel.clone();
    let on_key = move |e: Event<KeyboardEventData>| {
        if e.key == Key::Named(NamedKey::Escape) {
            keys.keep_rewrite();
        }
    };
    let (title, act, keeps) = texts(shown.kind);
    let closes = panel.clone();
    let close = icon_button(&p, icons::X, "Close", true, move |_| closes.keep_rewrite());
    let top = rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::flex())
        .cross_align(Alignment::Center)
        .child(rect().width(Size::flex(1.0)).child(head(&p, title)))
        .child(close);
    let action = button(&p, Look::Plain)
        .padding((2.0, 12.0))
        .child(label().text(act));
    let translation = shown.kind == BoxKind::Translation;
    let switch = shown.replaces.is_some();
    let (acts, opens) = (panel.clone(), panel.clone());
    let open = translation.then(|| {
        let b = button(&p, Look::Plain)
            .padding((2.0, 12.0))
            .child(label().text("Open in Blitznir"));
        pressable(b, move |_| opens.open_offer())
    });
    let row = rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::flex())
        .cross_align(Alignment::Center)
        .spacing(10.0)
        .child(pressable(action, move |_| match shown.kind {
            BoxKind::Translation => acts.copy_offer(),
            BoxKind::Later => acts.rewrite_offered(),
            BoxKind::Revert | BoxKind::Suggest => acts.revert(),
        }))
        .maybe_child(open)
        .child(
            rect().width(Size::flex(1.0)).child(
                label()
                    .text(keeps)
                    .font_size(11.0)
                    .color(p.muted)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis),
            ),
        );
    rect()
        .expanded()
        .background(p.bg)
        .color(p.fg)
        .font_size(12.0)
        .border(Border::new().fill(p.line).width(1.0))
        .padding(10.0)
        .spacing(6.0)
        .content(Content::flex())
        .on_global_key_down(on_key)
        .child(top)
        .child(
            ScrollView::new()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .child(Selectable::new().span(shown.text).width(Size::fill())),
        )
        .child(row)
        .maybe_child(switch.then(|| next_time(&p, panel, replaces)))
}

/// The box's title, its button and its hint.
fn texts(kind: BoxKind) -> (&'static str, &'static str, &'static str) {
    match kind {
        BoxKind::Suggest => (
            "Rewrite ready, your dictation is in place:",
            "Use rewrite",
            "Type on or click away: your dictation stays",
        ),
        BoxKind::Revert => (
            "Rewritten. Your dictation was:",
            "Revert",
            "Type on or click away: the rewrite stays",
        ),
        BoxKind::Translation => ("Translation:", "Copy", "Type on or click away to close"),
        BoxKind::Later => (
            "Dictated:",
            "Rewrite",
            "Type on or click away: it stays as said",
        ),
    }
}

/// Replace or Suggest for the next dictation, written to config.toml.
fn next_time(p: &Palette, panel: Arc<Panel>, mut replaces: State<bool>) -> Element {
    let switch = move |on: bool| {
        let panel = panel.clone();
        move |_| {
            replaces.set(on);
            panel.set_rewrite_replaces(on);
        }
    };
    let look = |on: bool| if on { Look::Plain } else { Look::Tool };
    rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(4.0)
        .child(label().text("Next time:").font_size(11.0).color(p.muted))
        .child(choice(p, look(replaces()), "Replace", switch(true)))
        .child(choice(p, look(!replaces()), "Suggest", switch(false)))
        .into_element()
}
