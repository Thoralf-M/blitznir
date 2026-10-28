//! The open chat's title in its head: the pencil makes it a field (Return
//! keeps what is typed, Esc takes it back), the wand asks the picked model
//! for a title from what was said.

use super::icons::{self, icon_button, icon_button_if};
use super::keys::field_keys;
use super::section::choice;
use super::style::{Look, Palette, selectable_line, use_palette};
use crate::panel::Panel;
use crate::panel::view::Ui;
use freya::prelude::*;
use std::sync::Arc;

#[derive(PartialEq)]
pub struct ChatTitle {
    pub name: String,
    /// The chat's id, its key: another chat's title starts unedited (the
    /// field kept the last one's name, and its Return renamed this one).
    pub id: String,
    /// The most its text may take; cut with an ellipsis beyond.
    pub max: f32,
}

impl Component for ChatTitle {
    fn render_key(&self) -> DiffKey {
        DiffKey::from(&self.id)
    }

    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let titling = use_consume::<State<Ui>>().read().titling;
        let mut editing = use_state(|| false);
        let mut typed = use_state(String::new);
        if editing() {
            return editor(&p, &panel, typed, editing);
        }
        let name = self.name.clone();
        let rename = move |_| {
            typed.set(name.clone());
            editing.set(true);
        };
        let suggest = panel.clone();
        let wand_tip = if titling {
            "Asking the model for a title…"
        } else {
            "Suggest a title: the picked model reads the chat"
        };
        rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(2.0)
            .child(
                selectable_line(self.name.clone())
                    .font_weight(FontWeight::SEMI_BOLD)
                    .max_width(Size::px(self.max)),
            )
            .child(icon_button(
                &p,
                icons::PENCIL,
                "Rename this chat",
                true,
                rename,
            ))
            .child(icon_button_if(
                &p,
                icons::WAND,
                wand_tip,
                (true, !titling),
                move |_| {
                    suggest.suggest_title();
                },
            ))
            .into_element()
    }
}

/// The title as a field: Return keeps what is typed, Esc takes it back.
fn editor(
    p: &Palette,
    panel: &Arc<Panel>,
    typed: State<String>,
    mut editing: State<bool>,
) -> Element {
    let (keep, keys) = (panel.clone(), panel.clone());
    let save = move |_| {
        if keep.rename_chat(&typed.read()) {
            editing.set(false);
        }
    };
    let on_keys = move |e: Event<KeyboardEventData>| {
        field_keys(&e, |e| match &e.key {
            Key::Named(NamedKey::Enter) => {
                if keys.rename_chat(&typed.read()) {
                    editing.set(false);
                }
                Some(false)
            }
            // the field's, not the window's: the panel stays open
            Key::Named(NamedKey::Escape) => {
                editing.set(false);
                Some(false)
            }
            _ => None,
        })
    };
    let field = Input::new(typed)
        .compact()
        .auto_focus(true)
        .width(Size::px(240.0))
        .background(p.field)
        .focus_background(p.field)
        .border_fill(p.line)
        .focus_border_fill(p.accent)
        .color(p.fg)
        .on_pre_key_down(on_keys);
    rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(4.0)
        .child(field)
        .child(choice(p, Look::Plain, "Save", save))
        .child(choice(p, Look::Tool, "Cancel", move |_| editing.set(false)))
        .into_element()
}
