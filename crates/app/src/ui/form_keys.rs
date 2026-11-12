//! A hotkey field with Record: the next key pressed goes in by its config
//! name (LeftShift, RightMeta, F13…), so nobody has to know them. The key
//! comes from the hotkeys' own listener (panel/form.rs), which tells left
//! from right for every modifier; only what a hotkey can be is taken: a
//! modifier, double-tapped, or an F-key.

use super::{
    keys::field_keys,
    style::{Look, Palette, button, pressable, use_palette},
};
use crate::panel::{Panel, form::Target, view::Ui};
use freya::prelude::*;
use std::sync::Arc;

/// The field and its Record button; `local` is what the field holds.
#[derive(PartialEq)]
pub struct KeysInput {
    pub local: State<String>,
    pub target: Target,
    pub error: bool,
}

impl Component for KeysInput {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let ui = use_consume::<State<Ui>>();
        let target = self.target;
        let recording = ui.read().recording.filter(|(t, _)| *t == target);
        let input = Input::new(self.local)
            .width(Size::flex(1.0))
            .background(p.field)
            .focus_background(p.field)
            .border_fill(if self.error { p.critical } else { p.line })
            .focus_border_fill(p.accent)
            .color(p.fg)
            .on_pre_key_down(move |e: Event<KeyboardEventData>| {
                // recording: the listener has the key, the field types nothing
                if recording.is_some() {
                    e.stop_propagation();
                    e.prevent_default();
                    return false;
                }
                field_keys(&e, |_| None)
            });
        let record = record_button(&p, recording.is_some(), move |_| {
            panel.record(recording.is_none().then_some(target));
        });
        let say = recording.map(|(_, refused)| {
            let text = if refused {
                "Not a hotkey: press a modifier (Shift, Ctrl, Option, Cmd, Fn) or F1 to F24"
            } else {
                "Press the key: a modifier (used by double-tapping) or an F-key"
            };
            label().text(text).font_size(12.0).color(p.muted)
        });
        rect()
            .width(Size::fill())
            .spacing(3.0)
            .child(
                rect()
                    .width(Size::fill())
                    .horizontal()
                    .content(Content::flex())
                    .cross_align(Alignment::Center)
                    .spacing(6.0)
                    .child(input)
                    .child(record),
            )
            .maybe_child(say)
    }
}

fn record_button(
    p: &Palette,
    on: bool,
    press: impl FnMut(Event<PressEventData>) + 'static,
) -> Rect {
    let (look, text) = if on {
        (Look::Default, "Cancel")
    } else {
        (Look::Plain, "Record")
    };
    pressable(
        button(p, look)
            .padding((3.0, 10.0))
            .child(label().text(text)),
        press,
    )
}
