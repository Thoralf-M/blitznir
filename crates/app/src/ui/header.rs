//! The header: fold the sidebar, the model picker, Settings. On macOS it is
//! the title bar too: the window's content runs under a see-through one
//! (panel/window.rs), the header leaves room for the traffic lights, and a
//! drag on its empty parts moves the window.

use super::icons::{self, icon, icon_button};
use super::style::{Look, Palette, button, line, press_keys, pressable, ring, use_palette};
use super::tip::Tip;
use super::zoom::zoom_drag;
use super::{NARROW, Peek};
use crate::panel::Panel;
use crate::panel::view::{MOD, ModelRow, Ui};
use freya::prelude::*;
use std::sync::Arc;

/// Whether the picker's list is open; the window's Esc closes it first.
#[derive(Clone, Copy)]
pub struct PickerOpen(pub State<bool>);

/// The traffic lights' room at the header's left on macOS.
const LIGHTS: f32 = if cfg!(target_os = "macos") { 76.0 } else { 0.0 };

/// Room the header keeps for the fold, Think and Settings buttons.
const OTHERS: f32 = 210.0;

/// A model as the picker shows it: its name, then " · endpoint" and the eye
/// when it accepts images. At most `room` wide: the name gives way in the
/// middle of the line, so the endpoint after it stays readable.
#[derive(PartialEq)]
struct ModelLine {
    row: ModelRow,
    room: f32,
}

impl Component for ModelLine {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let mut tail = use_state(|| 0.0_f32);
        let r = &self.row;
        let (name, color) = match (r.model.is_empty(), r.error) {
            (true, _) => (r.label.clone(), if r.error { p.muted } else { p.fg }),
            (false, _) => (r.model.clone(), p.fg),
        };
        let endpoint = (!r.model.is_empty()).then(|| {
            label()
                .text(format!(" · {}", r.endpoint))
                .color(p.muted)
                .max_lines(1)
        });
        let eye = r.vision.then(|| {
            Tip::new("accepts images").child(rect().padding((0.0, 0.0, 0.0, 5.0)).child(icon(
                icons::EYE,
                p.muted,
                14.0,
            )))
        });
        let suffix = rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .on_sized(move |e: Event<SizedEventData>| tail.set_if_modified(e.area.width()))
            .maybe_child(endpoint)
            .maybe_child(eye);
        rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .child(
                line(name)
                    .color(color)
                    .max_width(Size::px((self.room - tail()).max(40.0))),
            )
            .child(suffix)
    }
}

/// A row of the picker's list: Tab reaches it, Return or Space picks it.
#[derive(PartialEq)]
struct Choice {
    row: ModelRow,
    room: f32,
    /// The picked model's row.
    on: bool,
    press: EventHandler<Event<PressEventData>>,
}

impl Component for Choice {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let a11y = use_a11y();
        let keyed = use_focus(a11y)() == Focus::Keyboard;
        rect()
            .width(Size::fill_minimum())
            .padding((4.0, 8.0))
            .corner_radius(5.0)
            .a11y_id(a11y)
            .a11y_focusable(!self.row.error)
            .a11y_role(AccessibilityRole::ListItem)
            .background(if self.on || keyed {
                p.soft
            } else {
                Color::TRANSPARENT
            })
            .maybe(keyed, |r| r.border(ring(&p)))
            .on_press(self.press.clone())
            .on_key_down(press_keys(self.press.clone()))
            .child(ModelLine {
                row: self.row.clone(),
                room: self.room,
            })
    }
}

/// The picked model's row, or one that says there is none.
fn current(ui: &Ui) -> ModelRow {
    let (endpoint, model) = &ui.picked;
    let row = ui
        .models
        .iter()
        .find(|r| !r.error && &r.endpoint == endpoint && &r.model == model);
    row.cloned().unwrap_or_else(|| ModelRow {
        endpoint: endpoint.clone(),
        model: model.clone(),
        label: if model.is_empty() {
            "Pick a model".into()
        } else {
            format!("{model} · {endpoint}")
        },
        error: model.is_empty(),
        vision: false,
    })
}

/// The list under the picker: right-aligned to it, as wide as its longest
/// row up to the window's left edge; long lists scroll.
/// `shown` gets the list's own area: a press outside it closes the list.
fn list(
    p: &Palette,
    (panel, ui): (&Arc<Panel>, &Ui),
    at: Area,
    (mut open, mut shown): (State<bool>, State<Option<Area>>),
) -> Rect {
    let window = super::window_size();
    let room = at.max_x() - 8.0;
    let picked = current(ui);
    let rows = ui.models.iter().enumerate().map(|(i, r)| {
        let (panel, row) = (panel.clone(), r.clone());
        let on = !r.error && r.endpoint == picked.endpoint && r.model == picked.model;
        let press = EventHandler::new(move |_| {
            if !row.error {
                open.set(false);
                panel.set_model(&row);
            }
        });
        Tip::new(r.label.clone() + if r.vision { " · accepts images" } else { "" })
            .key(i)
            .child(Choice {
                row: r.clone(),
                room: room - 24.0,
                on,
                press,
            })
            .into_element()
    });
    // placed over the room from the window's left edge to the picker's
    // right one, not inside the picker: laid out in that, it keeps its width
    let box_ = rect()
        .on_sized(move |e: Event<SizedEventData>| shown.set_if_modified(Some(e.area)))
        .max_width(Size::px(room))
        .padding(4.0)
        .corner_radius(8.0)
        .border(Border::new().fill(p.line).width(1.0))
        .background(p.bg)
        .child(
            ScrollView::new()
                .width(Size::auto())
                .height(Size::auto())
                .max_height(Size::px((window.height - at.max_y() - 16.0).max(80.0)))
                .children(rows),
        );
    rect()
        .position(Position::new_global().top(at.max_y() + 4.0).left(8.0))
        .width(Size::px(room))
        .horizontal()
        .main_align(Alignment::End)
        .layer(Layer::Overlay)
        .child(box_)
}

/// The model picker: every endpoint's models; a dead endpoint is a grey
/// row that says why.
#[derive(PartialEq)]
struct Picker;

impl Component for Picker {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let ui = use_consume::<State<Ui>>();
        let ui = ui.read();
        let PickerOpen(mut open) = use_consume::<PickerOpen>();
        let mut at = use_state(|| None::<Area>);
        let list_at = use_state(|| None::<Area>);
        let window = super::window_size();
        let row = current(&ui);
        let tip = row.label.clone() + if row.vision { " · accepts images" } else { "" };
        let shown = button(&p, Look::Plain).child(ModelLine {
            row,
            room: (window.width - OTHERS - LIGHTS - 30.0).max(80.0),
        });
        let shown = pressable(shown, move |_| open.toggle());
        // a press anywhere else closes the list
        let outside = move |e: Event<PointerEventData>| {
            let c = e.global_location();
            let c = (c.x as f32, c.y as f32).into();
            let inside = |a: Option<Area>| a.is_some_and(|a| a.contains(c));
            if !inside(at()) && !inside(list_at()) {
                open.set_if_modified(false);
            }
        };
        let dropped = (open() && at().is_some()).then(|| {
            let area = at().unwrap_or_default();
            list(&p, (&panel, &ui), area, (open, list_at))
        });
        rect()
            .on_sized(move |e: Event<SizedEventData>| at.set_if_modified(Some(e.area)))
            .on_global_pointer_up(outside)
            // none over its own list
            .child(
                Tip::new(if open() {
                    String::new()
                } else {
                    tip + " · the model for rewriting and chat"
                })
                .child(shown),
            )
            .maybe_child(dropped)
    }
}

const THINK_ON: &str = "Thinking on: the model reasons before a chat answer (more complete, \
     slower). Click to turn it off";
const THINK_OFF: &str =
    "Thinking off: chat answers come at once. Click to let the model reason first";

/// Think: `[llm] chat_reasoning`, lit when on. Chat answers only: a
/// rewrite never reasons (it measurably hurts a spec-following edit).
fn think(p: &Palette, panel: &Arc<Panel>, on: bool) -> Tip {
    let panel = panel.clone();
    let (look, src, color, tip) = if on {
        (Look::Plain, icons::THINK, p.accent, THINK_ON)
    } else {
        (Look::Tool, icons::THINK_OFF, p.muted, THINK_OFF)
    };
    let b = button(p, look).child(
        rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(5.0)
            .child(icon(src, color, 16.0))
            .child(label().text("Think").color(if on { p.fg } else { color })),
    );
    Tip::new(tip).child(pressable(b, move |_| panel.set_chat_reasoning(!on)))
}

#[derive(PartialEq)]
pub struct Header;

impl Component for Header {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let ui = use_consume::<State<Ui>>();
        let ui = ui.read();
        let (fold, gear, back) = (panel.clone(), panel.clone(), panel.clone());
        let (folded, settings) = (ui.folded, ui.settings);
        let Peek(mut peek) = use_consume::<Peek>();
        let narrow = super::window_size().width < NARROW;
        let fold_tip = "Show or hide the sidebar: recent texts and saved chats";
        let before = use_state(|| None); // where a zoom goes back to
        rect()
            .width(Size::fill())
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(2.0)
            // its middle near the lights', 14 pt down
            .height(Size::px(32.0))
            .padding((0.0, 10.0, 0.0, 0.0))
            .content(Content::flex())
            // the empty bar moves the window, never the buttons: two quick
            // presses on one would be a double click, which zooms it
            .child(zoom_drag(
                rect().width(Size::px(8.0 + LIGHTS)).height(Size::fill()),
                before,
            ))
            .child(icon_button(
                &p,
                icons::SIDEBAR,
                fold_tip,
                false,
                move |_| {
                    if narrow {
                        peek.toggle(); // for as long as the window stays narrow
                    } else {
                        fold.set_folded(!folded);
                    }
                },
            ))
            .maybe_child(settings.then(|| {
                icon_button(
                    &p,
                    icons::BACK,
                    "Back to the chat (Esc)",
                    false,
                    move |_| {
                        back.show_settings(false);
                    },
                )
            }))
            .child(zoom_drag(
                rect().width(Size::flex(1.0)).height(Size::fill()),
                before,
            ))
            .maybe_child((!settings).then(|| think(&p, &panel, ui.reasons)))
            .child(Picker)
            .child(icon_button(
                &p,
                icons::GEAR,
                format!("Settings ({MOD}+,)"),
                false,
                move |_| {
                    gear.show_settings(!settings);
                },
            ))
    }
}
