//! Settings' About page: what this build is, where its data lives, and whose
//! work it stands on (the speech model's licence asks to be named).

use super::{
    form::card,
    select::Selectable,
    style::{Look, Palette, button_if, pressable},
};
use crate::panel::{Panel, updates::Updates};
use freya::prelude::*;
use std::sync::Arc;

/// The commit a release was built from (the bundle scripts set it); none
/// in a plain `cargo build`.
const COMMIT: Option<&str> = option_env!("BLITZNIR_COMMIT");

/// A small blank line between the card's groups.
fn gap() -> Span<'static> {
    Span::new("\n").font_size(5.0)
}

/// A group's heading inside the card's text, as `head` draws one.
fn heading(p: &Palette, text: &str) -> Span<'static> {
    Span::new(format!("{text}\n"))
        .font_size(11.0)
        .font_weight(FontWeight::SEMI_BOLD)
        .color(p.muted)
}

/// Check for updates and what it said: only on a press, never by itself.
fn updates(p: &Palette, panel: &Arc<Panel>, now: &Updates) -> Rect {
    let checking = *now == Updates::Checking;
    let ask = panel.clone();
    let check = pressable(
        button_if(p, Look::Plain, !checking).child(label().text("Check for updates")),
        move |_| {
            if !checking {
                ask.check_updates();
            }
        },
    );
    let said: Option<Element> = match now {
        Updates::Unasked => None,
        Updates::Checking => Some(label().text("Asking GitHub…").color(p.muted).into()),
        Updates::Latest => Some(
            label()
                .text("This is the latest version.")
                .color(p.muted)
                .into(),
        ),
        Updates::Failed(e) => Some(
            label()
                .text(format!("Could not check: {e}"))
                .color(p.critical)
                .into(),
        ),
        Updates::Newer { version, url } => {
            let (open, url) = (panel.clone(), url.clone());
            let link = button_if(p, Look::Default, true)
                .child(label().text(format!("Blitznir {version} is out: open its page")));
            Some(pressable(link, move |_| open.open_link(&url)).into())
        }
    };
    rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(10.0)
        .child(check)
        .maybe_child(said)
}

pub fn about(p: &Palette, panel: &Arc<Panel>, now: &Updates, data: String) -> Rect {
    let version = match COMMIT.filter(|c| !c.is_empty()) {
        Some(c) => format!("Blitznir {} ({c})", env!("CARGO_PKG_VERSION")),
        None => format!("Blitznir {}", env!("CARGO_PKG_VERSION")),
    };
    let credits = [
        "Speech: Parakeet Ultra by moondream (post-train of NVIDIA Parakeet TDT 0.6B v3), CC BY 4.0; ONNX by Masterx",
        "Interface: Freya, MIT; thunder: thunder-synth, Apache-2.0",
    ];
    // one text, headings and all, so a drag selects across it like a page
    let text = Selectable::new()
        .width(Size::fill())
        .span(
            Span::new("About\n")
                .font_size(15.0)
                .font_weight(FontWeight::BOLD),
        )
        .span(gap())
        .span(Span::new(format!("{version}\n")).font_weight(FontWeight::SEMI_BOLD))
        .span(gap())
        .span(
            "Local AI first: dictation runs on this computer; rewriting, translation \
             and chat go only to the server you picked.\n",
        )
        .span(gap())
        .span(heading(p, "Your data"))
        .span(Span::new(format!("{data}\n")).color(p.muted))
        .span(gap())
        .span(heading(p, "Credits"))
        .span(Span::new(credits.join("\n")).color(p.muted));
    card(p)
        .spacing(10.0)
        .child(text)
        .child(updates(p, panel, now))
}
