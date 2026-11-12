//! Settings: a Form tab (ui/form.rs) and a Text tab, config.toml in Freya's
//! code editor, highlighted by the TOML grammar: two views of one text, so
//! a whole config can be pasted in. Save validates (nothing is written if
//! it does not parse); Reset to defaults and Ask model only fill the editor.
//! The sidebar's About shows ui/about.rs in their place, with only Back.

use super::{
    keys::field_keys,
    select::Selectable,
    style::{Look, Palette, button, button_if, grown_height, pressable, syntax, use_palette},
    tip::Tip,
};
use crate::panel::{
    Panel,
    view::{MOD, RET, Ui},
};
use freya::{
    code_editor::{CodeEditor, CodeEditorData, EditorLanguage, Rope},
    prelude::*,
    text_edit::TextEditor,
};
use std::sync::Arc;

const FONT: &str = "monospace";
pub(super) const FONT_SIZE: f32 = 12.0;

fn toml() -> EditorLanguage {
    EditorLanguage::new(
        tree_sitter_toml_ng::LANGUAGE,
        tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
    )
}

/// The editor holding `text`, highlighted in `p`'s colours.
pub fn editor(text: &str, p: &Palette) -> CodeEditorData {
    let mut ed = CodeEditorData::new(Rope::from_str(text), toml());
    ed.set_theme(syntax(p));
    ed.parse();
    ed.measure(FONT_SIZE, FONT);
    ed
}

/// The whole text replaced as one edit, which the editor's undo takes back.
pub fn replace(ed: &mut CodeEditorData, text: &str) {
    let len = ed.len_utf16_cu();
    ed.remove(0..len);
    ed.insert(text, 0);
    ed.move_cursor_to(0);
    ed.parse();
    ed.measure(FONT_SIZE, FONT);
}

/// The editor's keys before it sees them: Tab indents, Esc and ⌘S are the window's.
fn keys(e: Event<KeyboardEventData>) -> bool {
    field_keys(&e, |e| {
        matches!(e.key, Key::Named(NamedKey::Tab)).then_some(true)
    })
}

/// A button with its tooltip above it.
fn tipped(tip: String, b: Rect) -> Tip {
    Tip::new(tip).top().child(b)
}

/// Back alone, under About: nothing there to save.
fn back(p: &Palette, panel: &Arc<Panel>) -> Rect {
    let back = panel.clone();
    rect()
        .width(Size::fill())
        .horizontal()
        .main_align(Alignment::End)
        .child(pressable(
            button(p, Look::Plain).child(label().text("Back")),
            move |_| back.show_settings(false),
        ))
}

/// About in the form's place: scrolls if the window is short.
fn about(p: &Palette, panel: &Arc<Panel>, ui: &Ui) -> ScrollView {
    let card = super::about::about(p, panel, &ui.updates, panel.data_dir());
    ScrollView::new()
        .width(Size::fill())
        .height(Size::flex(1.0))
        .child(
            rect()
                .width(Size::fill())
                .padding((8.0, 0.0, 0.0, 0.0))
                .child(card),
        )
}

/// What the last save said, then the text's buttons (the form saves itself:
/// only Back there).
fn row(p: &Palette, panel: &Arc<Panel>, ui: &Ui) -> Rect {
    let text_tab = !ui.form;
    let idle = ui.busy.is_none();
    let (run, reset, save, back) = (panel.clone(), panel.clone(), panel.clone(), panel.clone());
    let text = |t: &str| label().text(t.to_string());
    let ask_tip =
        format!("The picked model edits the text above ({RET}); nothing is saved until you Save");
    let reset_tip = "Load the default settings into the editor; nothing is saved until you Save";
    let save_button = pressable(
        button(p, Look::Default).child(text("Save").font_weight(FontWeight::SEMI_BOLD)),
        move |_| save.save_config(),
    );
    rect()
        .width(Size::fill())
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(6.0)
        .content(Content::flex())
        // what the last save said, left of the buttons: no line of its own
        .child(
            rect().width(Size::flex(1.0)).child(
                Selectable::new()
                    .span(ui.config.msg.clone())
                    .width(Size::fill())
                    .max_lines(2)
                    .text_overflow(TextOverflow::Ellipsis),
            ),
        )
        .maybe_child(text_tab.then(|| {
            tipped(
                ask_tip,
                pressable(
                    button_if(p, Look::Plain, idle).child(text("Ask model")),
                    move |_| run.ask_config(),
                ),
            )
        }))
        .maybe_child(text_tab.then(|| {
            tipped(
                reset_tip.into(),
                pressable(
                    button_if(p, Look::Plain, idle).child(text("Reset to defaults")),
                    move |_| reset.reset_config(),
                ),
            )
        }))
        .maybe_child(text_tab.then(|| tipped(format!("Save and apply ({MOD}+S)"), save_button)))
        .child(pressable(
            button(p, Look::Plain).child(text("Back")),
            move |_| back.show_settings(false),
        ))
}

/// The instruction field's keys: {RET} asks, Shift+{RET} is a new line,
/// Esc, Tab, ⌘S and ⌘, are the window's.
fn ask_keys(panel: Arc<Panel>) -> impl FnMut(Event<KeyboardEventData>) -> bool + 'static {
    move |e: Event<KeyboardEventData>| {
        field_keys(&e, |e| {
            let enter = matches!(e.key, Key::Named(NamedKey::Enter));
            (enter && !e.modifiers.contains(Modifiers::SHIFT)).then(|| {
                panel.ask_config();
                false
            })
        })
    }
}

/// The instruction for the model, full width over the buttons.
fn ask_field(p: &Palette, panel: &Arc<Panel>, ask: State<String>, width: State<f32>) -> Rect {
    let mut width = width;
    let height = grown_height(&ask.read(), *width.read(), (3.0, 8.0)).max(72.0);
    let placeholder = format!(
        "Tell the model what to change in the settings above, e.g. \"set double_tap_ms to 600\". \
         {RET} asks, Shift+{RET} starts a new line; nothing is saved until you Save"
    );
    rect()
        .width(Size::fill())
        .on_sized(move |e: Event<SizedEventData>| width.set_if_modified(e.area.width()))
        .child(
            Input::new(ask)
                .multiline(true)
                .placeholder(placeholder)
                .width(Size::fill())
                .height(Size::px(height))
                .background(p.field)
                .focus_background(p.field)
                .border_fill(p.line)
                .focus_border_fill(p.accent)
                .color(p.fg)
                .placeholder_color(p.muted)
                .on_pre_key_down(ask_keys(panel.clone())),
        )
}

/// Form | Text, and what both edit; under About neither is on, and a press
/// leaves it.
fn tabs(p: &Palette, panel: &Arc<Panel>, form: bool, mut about: State<bool>) -> Rect {
    let form = (!about()).then_some(form);
    let tab = |on: Option<bool>, text: &str, to: bool| {
        let (panel, on) = (panel.clone(), on == Some(true));
        let look = if on { Look::Default } else { Look::Plain };
        pressable(
            button(p, look)
                .padding((3.0, 12.0))
                .child(label().text(text.to_string())),
            move |_| {
                about.set_if_modified(false);
                panel.show_form(to);
            },
        )
    };
    rect()
        .width(Size::fill())
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(4.0)
        .child(tab(form, "Form", true))
        .child(tab(form.map(|f| !f), "config.toml", false))
        .child(
            label()
                .text("  Both edit config.toml: the form saves itself, the text with Save.")
                .font_size(12.0)
                .color(p.muted),
        )
}

/// `editor` is the window's editor, `ask` the instruction field's text.
#[derive(PartialEq)]
pub struct Settings {
    pub editor: State<CodeEditorData>,
    pub id: AccessibilityId,
    pub ask: State<String>,
}

impl Component for Settings {
    fn render(&self) -> impl IntoElement {
        let p = use_palette();
        let panel = use_consume::<Arc<Panel>>();
        let ui = use_consume::<State<Ui>>();
        let ui = ui.read();
        let width = use_state(|| 600.0);
        let nav = use_consume::<super::form_nav::FormNav>();
        if *nav.about.read() {
            return rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .padding((12.0, 16.0, 0.0, 16.0))
                .spacing(8.0)
                .content(Content::flex())
                .child(tabs(&p, &panel, ui.form, nav.about))
                .child(about(&p, &panel, &ui))
                .child(back(&p, &panel));
        }
        let editor = CodeEditor::new(self.editor, self.id)
            .font_family(FONT)
            .font_size(FONT_SIZE)
            .gutter(false)
            .show_whitespace(false)
            .scroll_controller(nav.text_scroll)
            .on_pre_key_down(keys);
        rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            // the window's content already ends 12 pt above its edge
            .padding((12.0, 16.0, 0.0, 16.0))
            .spacing(8.0)
            .content(Content::flex())
            .child(tabs(&p, &panel, ui.form, nav.about))
            .maybe_child(ui.form.then_some(super::form::Form))
            .maybe_child((!ui.form).then(|| {
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .padding(8.0)
                    .corner_radius(6.0)
                    .overflow(Overflow::Clip)
                    .background(p.field)
                    .border(Border::new().fill(p.line).width(1.0))
                    .child(editor)
            }))
            .maybe_child((!ui.form).then(|| ask_field(&p, &panel, self.ask, width)))
            .child(row(&p, &panel, &ui))
    }
}
