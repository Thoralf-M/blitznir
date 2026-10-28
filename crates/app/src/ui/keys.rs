//! Which keys a focused text field keeps and which it hands to the window.

use freya::prelude::*;

/// ⌘ on macOS, Ctrl elsewhere: the modifier of the panel's shortcuts.
pub fn command(m: Modifiers) -> bool {
    m.contains(if cfg!(target_os = "macos") {
        Modifiers::META
    } else {
        Modifiers::CONTROL
    })
}

/// Cmd + (or =), Cmd - and Cmd 0: the window's zoom, never a field's text.
pub fn zoom_key(m: Modifiers, c: &str) -> bool {
    command(m) && matches!(c, "+" | "=" | "-" | "0")
}

/// The window's keys in every field: Esc, Tab (focus), ⌘, ⌘S, ⌘F and the zoom.
fn window_key(e: &KeyboardEventData) -> bool {
    match &e.key {
        Key::Named(NamedKey::Escape | NamedKey::Tab) => true,
        Key::Character(c) => {
            command(e.modifiers) && matches!(c.as_str(), "," | "s" | "f")
                || zoom_key(e.modifiers, c)
        }
        _ => false,
    }
}

/// A field's pre-key-down answer (true = the field handles the key). `own`
/// sees the key first and claims the field's special ones (Enter submits,
/// Esc empties a search, Tab indents): `Some(answer)`, the key stopped
/// here. Then the window's keys pass; every other key is the field's alone.
pub fn field_keys(
    e: &Event<KeyboardEventData>,
    own: impl FnOnce(&Event<KeyboardEventData>) -> Option<bool>,
) -> bool {
    let answer = own(e);
    if answer.is_none() && window_key(e) {
        return false;
    }
    e.stop_propagation();
    e.prevent_default();
    answer.unwrap_or(true)
}
