//! The panel's look: the system's
//! light or dark, its accent colour, and mixes of text and background. The
//! one palette of its own is the Settings editor's syntax colours.

use super::select::Selectable;
use freya::code_editor::{
    CodeEditorThemeExt, EditorSyntaxTheme, EditorTheme, EditorThemePreference,
};
use freya::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub struct Palette {
    pub bg: Color,
    pub fg: Color,
    pub muted: Color,
    pub line: Color,
    pub soft: Color,
    pub side: Color,
    pub field: Color,
    pub accent: Color,
    /// An error's text: the system red, dark enough to read on the field.
    pub critical: Color,
    /// A warning's text: amber, dark enough to read on the background.
    pub warn: Color,
    pub dark: bool,
}

impl Palette {
    pub fn of(theme: &Theme) -> Palette {
        let c = &theme.colors;
        let dark = theme.name == "dark";
        let mix = |t: f32| Color::lerp(c.background, c.text_primary, t);
        Palette {
            bg: c.background,
            fg: c.text_primary,
            muted: mix(0.5),
            line: mix(0.16),
            soft: mix(0.07),
            side: mix(0.03),
            field: if dark { mix(0.05) } else { Color::WHITE },
            accent: c.primary,
            critical: if dark {
                Color::from_rgb(255, 105, 97)
            } else {
                Color::from_rgb(200, 30, 30)
            },
            warn: if dark {
                Color::from_rgb(255, 196, 90)
            } else {
                Color::from_rgb(160, 95, 0)
            },
            dark,
        }
    }
}

/// The palette of the theme in use; the component follows its changes.
pub fn use_palette() -> Palette {
    Palette::of(&use_theme().read())
}

/// The theme for the system's light or dark, with the editor's parts
/// sized for a 13 px panel.
pub fn theme(dark: bool) -> Theme {
    let mut theme = if dark {
        dark_theme().with_dark_code_editor()
    } else {
        light_theme().with_light_code_editor()
    };
    let p = Palette::of(&theme);
    let editor = EditorTheme {
        background: p.field,
        text: p.fg,
        ..if dark {
            EditorTheme::dark()
        } else {
            EditorTheme::light()
        }
    };
    theme.set("code_editor", EditorThemePreference::from(editor));
    theme
}

/// config.toml's colours: comments grey, [tables] purple, keys blue,
/// strings green, numbers and booleans orange.
pub fn syntax(p: &Palette) -> EditorSyntaxTheme {
    let rgb = Color::from_rgb;
    let (table, key, string, number) = if p.dark {
        (
            rgb(197, 160, 245),
            rgb(140, 190, 245),
            rgb(135, 214, 150),
            rgb(232, 180, 115),
        )
    } else {
        (
            rgb(126, 58, 181),
            rgb(27, 99, 176),
            rgb(38, 122, 69),
            rgb(184, 90, 20),
        )
    };
    let punct = Color::lerp(p.field, p.fg, 0.55);
    let base = if p.dark {
        EditorSyntaxTheme::dark()
    } else {
        EditorSyntaxTheme::light()
    };
    EditorSyntaxTheme {
        text: p.fg,
        comment: p.muted,
        type_: table,
        property: key,
        variable: key,
        string,
        string_special: string,
        number,
        boolean: number,
        constant: number,
        punctuation: punct,
        punctuation_bracket: punct,
        punctuation_delimiter: punct,
        operator: punct,
        ..base
    }
}

/// How a button looks: the default one is filled with the accent.
#[derive(Clone, Copy, PartialEq)]
pub enum Look {
    Plain,
    Default,
    /// Borderless and muted: the icons and the tools row.
    Tool,
}

pub fn button(p: &Palette, look: Look) -> Button {
    button_if(p, look, true)
}

/// [`button`], faded while `enabled` is false (opacity 0.45).
pub fn button_if(p: &Palette, look: Look, enabled: bool) -> Button {
    let fade = |c: Color| {
        if enabled {
            c
        } else {
            Color::lerp(c, p.bg, 0.55)
        }
    };
    let p = &Palette {
        fg: fade(p.fg),
        muted: fade(p.muted),
        soft: fade(p.soft),
        accent: fade(p.accent),
        ..*p
    };
    let b = Button::new()
        .padding((4.0, 10.0))
        .corner_radius(6.0)
        .enabled(enabled)
        .focus_border_fill(if look == Look::Default {
            p.fg
        } else {
            p.accent
        });
    match look {
        Look::Plain => b
            .background(p.soft)
            .hover_background(Color::lerp(p.soft, p.fg, 0.08))
            .border_fill(p.line)
            .color(p.fg),
        Look::Default => b
            .background(p.accent)
            .hover_background(Color::lerp(p.accent, Color::BLACK, 0.12))
            .border_fill(Color::TRANSPARENT)
            .color(if enabled {
                Color::WHITE
            } else {
                Color::lerp(Color::WHITE, p.bg, 0.4)
            }),
        Look::Tool => b
            .background(Color::TRANSPARENT)
            .hover_background(p.soft)
            .border_fill(Color::TRANSPARENT)
            .color(p.muted),
    }
}

/// The ring round a control Tab has focused: Button draws its own in
/// [`button_if`]'s colour, list rows use this.
pub fn ring(p: &Palette) -> Border {
    Border::new()
        .fill(p.accent)
        .width(2.0)
        .alignment(BorderAlignment::Inner)
}

/// Return or Space presses the focused control. Freya presses on them itself
/// only while it sees no screen reader; on macOS any accessibility client
/// asking for the window's tree counts as one for the rest of the run, and
/// then only VoiceOver's Ctrl+Alt+Space presses. This takes over exactly
/// then, so a key never presses twice.
pub fn press_keys(
    press: EventHandler<Event<PressEventData>>,
) -> impl FnMut(Event<KeyboardEventData>) + 'static {
    move |e: Event<KeyboardEventData>| {
        let key = match &e.key {
            Key::Named(NamedKey::Enter) => true,
            Key::Character(c) => c == " ",
            _ => false,
        };
        if key && !e.is_press_event() {
            e.stop_propagation();
            e.prevent_default();
            press.call(e.map(PressEventData::Keyboard));
        }
    }
}

/// `b` pressed by `press`: a click, or Return or Space while it has focus.
pub fn pressable(b: Button, press: impl Into<EventHandler<Event<PressEventData>>>) -> Rect {
    let press = press.into();
    rect()
        .on_key_down(press_keys(press.clone()))
        .child(b.on_press(press))
}

/// A small grey heading ("Recent", "Selected text · …").
pub fn head(p: &Palette, text: impl Into<String>) -> Label {
    label()
        .text(text.into())
        .font_size(11.0)
        .font_weight(FontWeight::SEMI_BOLD)
        .color(p.muted)
}

/// One line to select and copy (⌘C), cut with an ellipsis.
pub fn selectable_line(text: impl Into<String>) -> Selectable {
    Selectable::new()
        .span(text.into())
        .max_lines(1)
        .text_overflow(TextOverflow::Ellipsis)
}

/// One line, cut with an ellipsis.
pub fn line(text: impl Into<String>) -> Label {
    label()
        .text(text.into())
        .max_lines(1)
        .text_overflow(TextOverflow::Ellipsis)
}

/// A multiline field's height for `text` at `width`: `lines.0` lines
/// empty, growing with its wrapped lines (estimated, 13 px text) to
/// `lines.1`, then the input scrolls itself (it keeps the caret's line in
/// view). Growing shows a short text whole, with no scrollbar.
pub fn grown_height(text: &str, width: f32, (min, max): (f32, f32)) -> f32 {
    const LINE: f32 = 18.0;
    const CHAR: f32 = 7.0;
    let per_line = ((width - 16.0) / CHAR).max(1.0);
    let wrapped = |l: &str| (l.chars().count() as f32 / per_line).ceil().max(1.0);
    let lines: f32 = text.split('\n').map(wrapped).sum();
    lines.clamp(min, max) * LINE + 16.0
}
