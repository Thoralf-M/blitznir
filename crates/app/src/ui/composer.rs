//! The composer: the attached images, the text over its button row. Return
//! runs the default button (the first action until a chat is entered, then
//! Send); Shift+Return starts a new line. Above it, the activity strip.

use super::dictation::{Dictation, PAD, use_wave};
use super::icons::{self, icon, icon_button, icon_button_if};
use super::keys::{command, field_keys};
use super::select::Selectable;
use super::style::{Look, Palette, button_if, pressable, use_palette};
use super::thumbs::{Shown, thumbs};
use super::tip::Tip;
use crate::panel::Panel;
use crate::panel::view::{self, MOD, RET, Strip, Tone, Ui, View};
use blitznir_core::tasks::{self, Kind};
use freya::prelude::*;
use std::sync::Arc;

/// The composer's keys before the field sees them: false = not for the field.
fn keys(panel: Arc<Panel>) -> impl FnMut(Event<KeyboardEventData>) -> bool + 'static {
    move |e: Event<KeyboardEventData>| {
        field_keys(&e, |e| match &e.key {
            Key::Named(NamedKey::Enter) if !e.modifiers.contains(Modifiers::SHIFT) => {
                panel.submit();
                Some(false)
            }
            Key::Character(c)
                if command(e.modifiers)
                    && c.eq_ignore_ascii_case("z")
                    && panel.undo_composer(e.modifiers.contains(Modifiers::SHIFT)) =>
            {
                Some(false)
            }
            Key::Character(c) if command(e.modifiers) && c == "v" => {
                panel.paste_image(true); // an image rides along; text pastes as ever
                Some(true)
            }
            _ => None,
        })
    }
}

fn placeholder(ui: &Ui) -> String {
    if view::viewing(ui) {
        return "Ask about this text…".into();
    }
    let name = |i: Option<usize>| i.and_then(|i| ui.actions.get(i)).map(String::as_str);
    if ui.chatting {
        return match name(ui.chat_enter) {
            Some(b) => format!("Ask anything: {RET} runs {b}, Shift+{RET} starts a new line"),
            None => format!("Ask anything: {RET} sends, Shift+{RET} starts a new line"),
        };
    }
    match name(ui.enter) {
        Some(b) => format!("{RET} runs {b} on the selection, or type an instruction first"),
        None => format!("{RET} starts a chat about the selection"),
    }
}

fn text_button(p: &Palette, look: Look, text: String, enabled: bool) -> Button {
    let weight = if look == Look::Default {
        FontWeight::SEMI_BOLD
    } else {
        FontWeight::NORMAL
    };
    button_if(p, look, enabled).child(label().text(text).font_weight(weight).max_lines(1))
}

/// Tooltips open upwards down here.
fn tip(text: impl Into<String>, child: impl IntoElement) -> Tip {
    Tip::new(text).top().child(child)
}

/// What Translate will do now, as core decides it: the selection, else
/// what is typed here, else nothing yet.
fn translate_tip(ui: &Ui) -> String {
    let (selection, typed) = (ui.selection.as_deref(), ui.composer.text.as_str());
    if tasks::translates_typed(Kind::Translate, selection, typed, ui.images.len()) {
        "Translate what you typed, as a new chat: the original above its translation".into()
    } else if selection.is_some_and(|s| !s.trim().is_empty()) {
        "Translate the selection: replaced in place, or shown here where the app takes no text"
            .into()
    } else {
        "Select text in an app, or type it here, to translate".into()
    }
}

/// Translate, one button per [[actions]] row, Chat/Send.
fn buttons(p: &Palette, panel: &Arc<Panel>, ui: &Ui) -> Vec<Element> {
    let idle = ui.busy.is_none();
    let out = View::writes_out(ui);
    let mut row = vec![];
    let add = panel.clone();
    let nav = use_consume::<super::form_nav::FormNav>();
    let tip = "Your buttons: add, edit or remove them in Settings (written to config.toml)";
    row.push(
        icon_button(p, icons::PLUS, tip, false, move |_| {
            add.open_buttons();
            nav.go(0); // the Buttons table, first in the form
        })
        .top()
        .into(),
    );
    let tr = panel.clone();
    let tip = translate_tip(ui);
    let translate = pressable(
        text_button(p, Look::Plain, "Translate".into(), idle),
        move |_| tr.run(Kind::Translate, 0),
    );
    row.push(self::tip(tip, translate).into());
    // the button Return runs here is the marked one
    let returns = if ui.chatting { ui.chat_enter } else { ui.enter };
    for (i, name) in ui.actions.iter().enumerate() {
        let first = i == 0;
        let look = if returns == Some(i) {
            Look::Default
        } else {
            Look::Plain
        };
        let (text, tip) = if first && out {
            let tip = "Paste the newest answer into the app you came from".to_string();
            ("Write out".to_string(), tip)
        } else {
            let key = if returns == Some(i) {
                format!(" ({RET})")
            } else {
                String::new()
            };
            let tip = format!("{name}: on the selection; what you typed is the instruction{key}");
            (name.clone(), tip)
        };
        let act = panel.clone();
        let b = pressable(text_button(p, look, text, idle), move |_| act.action(i));
        row.push(self::tip(tip, b).key(i).into());
    }
    let chat = panel.clone();
    let (look, key) = if returns.is_none() {
        (Look::Default, format!(" ({RET})"))
    } else {
        (Look::Plain, String::new())
    };
    let (label, tip) = if ui.chatting {
        ("Send", format!("Send{key}; Shift+{RET} starts a new line"))
    } else {
        let tip =
            format!("Start a chat about the selection; what you typed is the first message{key}");
        ("Chat", tip)
    };
    let b = pressable(text_button(p, look, label.into(), idle), move |_| {
        chat.chat_clicked();
    });
    row.push(self::tip(tip, b).into());
    row
}

/// What the tools take besides the buttons: Screenshot with its label, or
/// its icon alone, then the paste icon, the gaps and the row's padding.
const LEFT_FULL: f32 = 150.0;
const LEFT_ICON: f32 = 80.0;

/// When the row is too narrow for everything (a narrow window, or a wide
/// one with the sidebar open) the Screenshot button keeps only its icon and
/// tooltip, and narrower still the buttons wrap onto a second row.
fn tools(p: &Palette, panel: &Arc<Panel>, ui: &Ui) -> Rect {
    let (cap, img) = (panel.clone(), panel.clone());
    let idle = ui.busy.is_none();
    let mut room = use_state(|| super::window_size().width);
    let mut wanted = use_state(|| 0.0_f32);
    let icon_only = room() < wanted() + LEFT_FULL;
    let wrap = room() < wanted() + LEFT_ICON;
    let shot = button_if(p, Look::Tool, idle).padding((4.0, 6.0)).child(
        rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(5.0)
            .child(icon(
                icons::CAMERA,
                if idle { p.muted } else { p.line },
                16.0,
            ))
            .maybe_child((!icon_only).then(|| label().text("Screenshot"))),
    );
    let shot = pressable(shot, move |_| cap.capture());
    let tip = "Capture screen: select a region for your next message (Esc cancels)";
    let paste_tip = format!("Paste an image from the clipboard ({MOD}+V in the text field)");
    // the buttons' own width, in one line on the tools' row or below it
    let group = rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(4.0)
        .on_sized(move |e: Event<SizedEventData>| wanted.set_if_modified(e.area.width()))
        .children(buttons(p, panel, ui));
    rect()
        .width(Size::fill())
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(4.0)
        .padding((4.0, 6.0, 6.0, 6.0))
        .on_sized(move |e: Event<SizedEventData>| room.set_if_modified(e.area.width()))
        .content(if wrap {
            Content::wrap()
        } else {
            Content::flex()
        })
        .child(self::tip(tip, shot))
        .child(
            icon_button_if(p, icons::IMAGE, paste_tip, (false, idle), move |_| {
                img.paste_image(false);
            })
            .top(),
        )
        .maybe_child((!wrap).then(|| rect().width(Size::flex(1.0))))
        .child(group)
}

/// One line above the composer, only while there is something to say: a job
/// running, a confirmation, a warning; an error in red with an x.
pub fn activity(p: &Palette, panel: &Arc<Panel>, s: &Strip) -> Rect {
    let color = match s.tone {
        Tone::Quiet => p.muted,
        Tone::Warn => p.warn,
        Tone::Error => p.critical,
    };
    // selectable: an error is worth copying (a log line, a bug report)
    let text = Selectable::new()
        .span(s.text.clone())
        .width(Size::fill())
        .font_size(12.0)
        .color(color);
    let close = (s.tone == Tone::Error).then(|| {
        let panel = panel.clone();
        icon_button(p, icons::X, "Dismiss", true, move |_| panel.dismiss()).top()
    });
    rect()
        .width(Size::fill())
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(4.0)
        .padding((0.0, 14.0, 4.0, 16.0))
        .content(Content::flex())
        .child(rect().width(Size::flex(1.0)).child(text))
        .maybe_child(close)
}

/// The composer starts one line tall and grows with its text up to
/// [`COMPOSER_LINES`]; past them it scrolls.
const MIN_LINES: f32 = 1.0;
const COMPOSER_LINES: f32 = 10.0;
/// A line.
const LINE: f32 = 18.0;

/// The field's height for text `measured` tall: it grows to
/// [`COMPOSER_LINES`], then the input scrolls itself.
fn height(measured: f32) -> f32 {
    measured.clamp(LINE * MIN_LINES, LINE * COMPOSER_LINES) + 2.0 * PAD
}

/// The text laid out as the input lays it out, unseen, for its height and
/// the dictation wave's caret (`holder`).
fn measure(text: &str, mut into: State<f32>, holder: ParagraphHolder) -> Rect {
    // padding, not a margin: a margin does not narrow a filling paragraph;
    // the input's scrollbar lies over its text and takes none of its room
    let copy = paragraph()
        .holder(holder)
        .span(format!("{text} "))
        .width(Size::fill())
        .color(Color::TRANSPARENT)
        .on_sized(move |e: Event<SizedEventData>| into.set_if_modified(e.area.height()));
    let beside = rect().width(Size::fill()).padding((0.0, PAD)).child(copy);
    rect()
        .width(Size::fill())
        .height(Size::px(0.0))
        .overflow(Overflow::Clip)
        .child(beside)
}

/// `text` is the window's copy of what is typed; `id` lets the window focus it.
#[derive(PartialEq)]
pub struct Composer {
    pub text: State<String>,
    pub id: AccessibilityId,
}

impl Component for Composer {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let ui = use_consume::<State<Ui>>();
        let ui = ui.read();
        let measured = use_state(|| 0.0f32);
        let tall = height(*measured.read());
        let focused = *Platform::get().current_window().is_app_focused.read();
        let (wave, waving) = use_wave(ui.dictation_wave && ui.dictating && focused);
        let text = self.text.read();
        wave.end.set(text.encode_utf16().count());
        let hint = placeholder(&ui);
        let shown = if text.is_empty() { &hint } else { &*text };
        let input = Input::new(self.text)
            .multiline(true)
            .a11y_id(self.id)
            .auto_focus(true)
            .placeholder(hint.clone())
            .width(Size::fill())
            .height(Size::px(tall))
            .background(Color::TRANSPARENT)
            .focus_background(Color::TRANSPARENT)
            .border_fill(Color::TRANSPARENT)
            .focus_border_fill(Color::TRANSPARENT)
            .color(p.fg)
            .placeholder_color(p.muted)
            .on_pre_key_down(keys(panel.clone()));
        let large = use_consume::<Shown>();
        let attached = thumbs(&p, &ui.images, (&panel, true), large).map(|t| {
            rect()
                .width(Size::fill())
                .padding((8.0, 10.0, 0.0, 10.0))
                .child(t)
        });
        let report = panel.clone();
        let at = move |e: Event<SizedEventData>| {
            let a = e.area;
            let (x, y) = (f64::from(a.min_x()), f64::from(a.min_y()));
            let (w, h) = (f64::from(a.width()), f64::from(a.height()));
            report.composer_at(blitznir_core::platform::Rect { x, y, w, h });
        };
        rect()
            .width(Size::fill())
            .margin((0.0, 12.0))
            .on_sized(at)
            .corner_radius(10.0)
            .border(Border::new().fill(p.line).width(1.0))
            .background(p.field)
            .maybe_child(attached)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(tall))
                    .child(input)
                    // empty, the placeholder sets the height: never cut off
                    .child(measure(shown, measured, wave.holder.clone()))
                    .maybe_child(waving.then_some(Dictation)),
            )
            .child(tools(&p, &panel, &ui))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_translate_tip_says_what_a_press_will_translate() {
        let mut ui = Ui {
            selection: Some("Guten Morgen".into()),
            ..Ui::default()
        };
        assert!(translate_tip(&ui).starts_with("Translate the selection"));
        ui.selection = Some(String::new());
        assert!(translate_tip(&ui).starts_with("Select text"), "nothing yet");
        ui.composer.set("Ein Nest");
        assert!(translate_tip(&ui).starts_with("Translate what you typed"));
    }
}
