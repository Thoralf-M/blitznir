//! The panel window's components, one view: the header, the sidebar, the
//! band (the selection until a chat is entered, then the transcript) or the
//! Settings editor, the activity strip and the composer. They render the
//! panel's [`Ui`] and call the panel; what they keep themselves is only what
//! a text field holds while it is typed in, mirrored into the panel.

mod about;
mod answer;
mod chat_title;
mod composer;
mod dictation;
mod find;
mod follow;
mod form;
mod form_buttons;
mod form_keys;
mod form_nav;
mod form_page;
mod header;
mod icons;
mod keys;
mod opened;
pub mod revert;
mod section;
mod select;
mod settings;
mod sidebar;
mod splitter;
mod style;
mod thinking;
mod thumbs;
mod tip;
mod transcript;
mod zoom;

use crate::panel::Panel;
use crate::panel::view::Ui;
use composer::{Composer, activity};
use freya::prelude::*;
use keys::command;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use style::Palette;

/// Below this window width the sidebar folds away by itself.
pub const NARROW: f32 = 560.0;

/// The window's size in logical pixels, the unit of layout and of `on_sized`.
pub fn window_size() -> Size2D {
    *Platform::get().current_window().root_size.read()
}

/// The sidebar shown over a narrow window by its toggle; not saved.
#[derive(Clone, Copy)]
pub struct Peek(pub State<bool>);

/// A RenderCallback's canvas: it is also called to paint text, with none.
pub(crate) fn canvas<'a>(ctx: &'a FillRenderContext) -> Option<&'a freya::engine::prelude::Canvas> {
    match &ctx.target {
        RenderTarget::Background { canvas, .. } => Some(canvas),
        RenderTarget::TextStyle(_) => None,
    }
}

pub(crate) fn dark() -> bool {
    matches!(
        *Platform::get().current_window().preferred_theme.read(),
        PreferredTheme::Dark
    )
}

/// The panel's changes, from any thread, into this window's copy of [`Ui`].
fn use_bridge(panel: &Arc<Panel>) -> State<Ui> {
    let mut ui = use_state(|| panel.ui());
    let panel = panel.clone();
    use_hook(move || {
        let (tx, rx) = async_channel::unbounded();
        panel.attach(tx);
        spawn(async move {
            while rx.recv().await.is_ok() {
                while rx.try_recv().is_ok() {}
                ui.set_if_modified(panel.ui());
            }
        });
    });
    ui
}

/// The window's text fields and the panel's: what is typed goes to the
/// panel; what the panel sets (a new `rev`) comes into the field.
fn use_fields(panel: &Arc<Panel>, ui: State<Ui>, theme: State<Theme>) -> Fields {
    let first = ui.peek().clone();
    let pal = Palette::of(&theme.peek());
    let mut composer = use_state(|| first.composer.text.clone());
    let mut ask = use_state(|| first.ask.text.clone());
    let mut editor = use_state(|| settings::editor(&first.config.text.text, &pal));
    let seen = use_hook(|| {
        let revs = (first.composer.rev, first.ask.rev, first.config.text.rev);
        Rc::new(Cell::new(revs))
    });
    use_side_effect(move || {
        let u = ui.read();
        let (c, a, e) = seen.get();
        if u.composer.rev != c {
            composer.set(u.composer.text.clone());
        }
        if u.ask.rev != a {
            ask.set(u.ask.text.clone());
        }
        if u.config.text.rev != e {
            let text = &u.config.text.text;
            if u.config.undoable {
                settings::replace(&mut editor.write(), text);
            } else {
                editor.set(settings::editor(text, &Palette::of(&theme.peek())));
            }
        }
        seen.set((u.composer.rev, u.ask.rev, u.config.text.rev));
    });
    use_side_effect(move || {
        let pal = Palette::of(&theme.read());
        let mut ed = editor.write();
        ed.set_theme(style::syntax(&pal));
        ed.parse();
    });
    let p = panel.clone();
    use_side_effect(move || p.set_composer(composer.read().clone()));
    let p = panel.clone();
    use_side_effect(move || p.set_ask(ask.read().clone()));
    let p = panel.clone();
    use_side_effect(move || p.edit_config(editor.read().rope.to_string()));
    Fields {
        composer,
        ask,
        editor,
    }
}

struct Fields {
    composer: State<String>,
    ask: State<String>,
    editor: State<freya::code_editor::CodeEditorData>,
}

/// Focus follows the open: the composer, or the editor in Settings; a hidden
/// window lets go of it.
fn use_focus_follows(ui: State<Ui>, composer: AccessibilityId, editor: AccessibilityId) {
    let last = use_hook(|| Rc::new(Cell::new((0, false, false, false))));
    use_side_effect(move || {
        let u = ui.read();
        let now = (u.opened, u.chatting, u.settings, u.shown);
        if last.get() == now {
            return;
        }
        last.set(now);
        match (u.shown, u.settings) {
            (false, _) => {
                composer.request_unfocus();
                editor.request_unfocus();
            }
            (true, true) => editor.request_focus(),
            (true, false) => composer.request_focus(),
        }
    });
}

/// The window's own keys, whichever field has the focus.
fn window_keys(
    keys: Arc<Panel>,
    mut picking: State<bool>,
    ui: State<Ui>,
    (nav, mut peek): (form_nav::FormNav, State<bool>),
) -> impl FnMut(Event<KeyboardEventData>) + 'static {
    move |e: Event<KeyboardEventData>| match &e.key {
        Key::Named(NamedKey::Escape) if picking() => picking.set(false),
        Key::Named(NamedKey::Escape) => keys.escape(),
        Key::Character(c) if command(e.modifiers) && c == "," => keys.show_settings(true),
        Key::Character(c) if keys::zoom_key(e.modifiers, c) => zoom(&keys, c),
        Key::Character(c) if command(e.modifiers) && c == "s" && ui.peek().settings => {
            keys.save_config();
        }
        // the sidebar's search, Settings' or the lists': the sidebar shown
        // first (a narrow window's over the content); a field made for it
        // focuses itself as it appears (`find`), one already there now
        Key::Character(c) if command(e.modifiers) && c == "f" => {
            let narrow = window_size().width < NARROW;
            let hidden = if narrow { !peek() } else { ui.peek().folded };
            if narrow {
                peek.set(true);
            } else if hidden {
                keys.set_folded(false);
            }
            if hidden {
                let mut find = nav.find;
                find.set(true);
            } else {
                nav.search_id.request_focus();
            }
        }
        _ => {}
    }
}

/// Cmd + / - / 0: the whole window scales, as a browser's page does.
fn zoom(panel: &Arc<Panel>, key: &str) {
    let up = match key {
        "0" => None,
        "-" => Some(false),
        _ => Some(true),
    };
    Platform::get()
        .current_window()
        .set_custom_scale_factor(panel.zoom(up));
}

/// Wayland tells a window no theme: the desktop's, asked once as the
/// window is made (every show on Wayland makes it anew).
fn use_desktop_theme(panel: &Arc<Panel>) {
    let panel = panel.clone();
    use_hook(move || {
        if let Some(d) = panel.prefers_dark() {
            let t = if d {
                PreferredTheme::Dark
            } else {
                PreferredTheme::Light
            };
            Platform::get().current_window().preferred_theme.set(t);
        }
    });
}

pub fn app(panel: Arc<Panel>) -> impl IntoElement {
    let panel = use_provide_context(|| panel);
    let ui = use_bridge(&panel);
    use_provide_context(|| ui);
    use_desktop_theme(&panel);
    let mut theme = use_init_theme(|| style::theme(dark()));
    use_side_effect(move || theme.set(style::theme(dark())));
    let fields = use_fields(&panel, ui, theme);
    let (composer_id, editor_id) = (use_a11y(), use_a11y());
    use_focus_follows(ui, composer_id, editor_id);
    let picking = use_state(|| false);
    use_provide_context(|| header::PickerOpen(picking));
    let mut peek = use_state(|| false);
    let drag = use_state(|| None::<f32>);
    let nav = form_nav::use_form_nav(fields.editor);
    use_provide_context(|| Peek(peek));
    let width = window_size().width;
    use_side_effect(move || {
        if window_size().width >= NARROW {
            peek.set_if_modified(false);
        }
    });
    let on_key = window_keys(panel.clone(), picking, ui, (nav.clone(), peek));
    let p = Palette::of(&theme.read());
    let u = ui.read();
    let side = if width < NARROW { peek() } else { !u.folded };
    let rule = |w: Size, h: Size| rect().width(w).height(h).background(p.line);
    let center: Element = if u.settings {
        let editor = fields.editor;
        settings::Settings {
            editor,
            id: editor_id,
            ask: fields.ask,
        }
        .into_element()
    } else {
        transcript::Band.into_element()
    };
    let composer = (!u.settings).then_some(Composer {
        text: fields.composer,
        id: composer_id,
    });
    // under the band, which gives it the room: the composer's edge stays put
    let strip = u.strip.as_ref().map(|s| activity(&p, &panel, s));
    let content = rect()
        .width(Size::flex(1.0))
        .height(Size::fill())
        .padding((0.0, 0.0, 12.0, 0.0))
        .content(Content::flex())
        .child(center)
        .maybe_child(strip)
        .maybe_child(composer);
    let form_open = u.settings && u.form && !*nav.about.read();
    let body = rect()
        .width(Size::fill())
        .height(Size::flex(1.0))
        .on_wheel(move |e: Event<WheelEventData>| {
            // the form scrolls from anywhere in Settings: its section list,
            // the margins; over the form its own scroll view takes the wheel
            if form_open {
                let mut scroll = nav.scroll;
                let (_, y): (i32, i32) = scroll.into();
                scroll.scroll_to_y(y + e.delta_y as i32);
            }
        })
        .horizontal()
        .content(Content::flex())
        .children(splitter::side(
            side,
            (u.side_width, u.settings),
            drag,
            width < NARROW,
        ))
        .child(content);
    rect()
        .expanded()
        .background(p.bg)
        .color(p.fg)
        .font_size(13.0)
        .content(Content::flex())
        .on_global_key_down(on_key)
        .child(header::Header)
        .child(rule(Size::fill(), Size::px(1.0)))
        .child(body)
}
