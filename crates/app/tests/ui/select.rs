//! Selecting text in the Settings: from beside a line, across the About
//! card's lines and headings, and the pointer that shows where text is.

use super::kit::*;
use blitznir_app::ui;
use freya::prelude::{Area, CursorIcon, LaunchProxy, Size2D};
use freya_testing::prelude::*;

/// Settings' About page, from its sidebar entry.
fn about(proxy: &LaunchProxy) -> TestingRunner {
    let panel = panel(proxy, "about-select", &[]);
    panel.show_settings(true);
    let app = move || ui::app(panel.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    press_label(&mut t, "About");
    t
}

/// A drag that starts beside the text, right of a short line, still selects:
/// the text's whole row takes the press, not only its glyphs.
pub fn a_drag_from_beside_the_text_selects(proxy: &LaunchProxy) {
    const HELP: &str = "Up to 6, shown under the text field";
    let panel = panel(proxy, "beside", &[]);
    panel.show_settings(true);
    let app = move || ui::app(panel.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    let (_, at) = paragraph_with(&t, HELP).expect("the help");
    let y = f64::from(at.center().y);
    let beside = (f64::from(at.min_x()) + 400.0, y); // past the line's ~200 px of glyphs
    let start = (f64::from(at.min_x()) + 5.0, y);
    t.move_cursor(beside);
    t.press_cursor(beside);
    settle(&mut t); // the press focuses the text, which the drag needs
    t.move_cursor(start);
    t.release_cursor(start);
    settle(&mut t);
    let (lit, _) = paragraph_with(&t, HELP).expect("still there");
    assert!(!lit.is_empty(), "the drag from beside the text selected it");
}

/// The pointer is an I-beam over a line's glyphs and an arrow beside them.
pub fn the_pointer_is_a_beam_only_over_text(proxy: &LaunchProxy) {
    const HELP: &str = "Up to 6, shown under the text field";
    let panel = panel(proxy, "beam", &[]);
    panel.show_settings(true);
    let app = move || ui::app(panel.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    let (_, at) = paragraph_with(&t, HELP).expect("the help");
    let y = f64::from(at.center().y);
    t.move_cursor((f64::from(at.min_x()) + 20.0, y));
    settle(&mut t);
    assert_eq!(t.cursor_icon(), CursorIcon::Text, "over the glyphs");
    t.move_cursor((f64::from(at.min_x()) + 400.0, y));
    settle(&mut t);
    assert_eq!(t.cursor_icon(), CursorIcon::Default, "beside them");
}

/// Tab from one Settings field reaches the next field: the label and help
/// between them are text to select, not stops on the way. A press on that
/// text still gives it the keys, ⌘C's (⌘A here: it leaves the clipboard).
pub fn tab_goes_from_field_to_field(proxy: &LaunchProxy) {
    use blitznir_core::config::form::{field, value};
    const HELP: &str = "Text in it becomes the other one";
    let panel = panel(proxy, "tab-fields", &[]);
    panel.show_settings(true);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    press_label(&mut t, "Translate");
    // a press and its release frames apart, as a hand's: the focus moves
    // between them
    let press = |t: &mut TestingRunner, at: Area, x: f64| {
        let c = (f64::from(at.min_x()) + x, f64::from(at.center().y));
        t.move_cursor(c);
        t.press_cursor(c);
        settle(t);
        t.release_cursor(c);
        settle(t);
    };
    let (_, at) = paragraph_with(&t, "German").expect("Your language's field");
    press(&mut t, at, 20.0);
    t.press_key(Key::Named(NamedKey::Tab));
    settle(&mut t);
    t.write_text("XYZ");
    settle(&mut t);
    let other = field("translate", "other").expect("in the form");
    let text = panel.ui().config.text.text;
    assert!(
        value(&text, other).contains("XYZ"),
        "Tab went on to Other language: {:?}",
        value(&text, other)
    );
    let (_, at) = paragraph_with(&t, HELP).expect("the help");
    press(&mut t, at, 10.0);
    let command = if cfg!(target_os = "macos") {
        Modifiers::META
    } else {
        Modifiers::CONTROL
    };
    t.send_event(PlatformEvent::Keyboard {
        name: KeyboardEventName::KeyDown,
        key: Key::Character("a".into()),
        code: Code::KeyA,
        modifiers: command,
    });
    settle(&mut t);
    let (lit, _) = paragraph_with(&t, HELP).expect("still there");
    assert!(!lit.is_empty(), "the help took ⌘A: all of it selected");
}

/// A drag from the middle of the About card up past it selects everything
/// before the press, the headings included: one text, as on a page, not
/// each line to itself, and above the text means its start, not its column.
pub fn a_drag_up_through_about_selects_its_headings(proxy: &LaunchProxy) {
    let mut t = about(proxy);
    let (_, at) = paragraph_with(&t, "blitznir-ui-about").expect("the about text");
    let (lit, _) = paragraph_with(&t, "blitznir-ui-about").expect("the about text");
    assert!(lit.is_empty(), "nothing selected yet");
    let end = (f64::from(at.max_x()) - 5.0, f64::from(at.center().y));
    t.move_cursor(end);
    t.press_cursor(end);
    settle(&mut t); // the press focuses the text, which the drag needs
    let above = (f64::from(at.max_x()) - 5.0, f64::from(at.min_y()) - 30.0);
    t.move_cursor(above);
    t.release_cursor(above);
    settle(&mut t);
    shot(&mut t, "about-drag-up");
    let (lit, _) = paragraph_with(&t, "blitznir-ui-about").expect("still there");
    let from = lit.iter().map(|h| h.0.min(h.1)).min();
    assert_eq!(
        from,
        Some(0),
        "the selection reaches the card's first word: {lit:?}"
    );
}

/// An error above the composer can be selected and copied, like any text.
pub fn an_error_above_the_composer_selects(proxy: &LaunchProxy) {
    const ERR: &str = "Chat failed: the answer was cut off at max_tokens=8192";
    let panel = panel(proxy, "error-select", &[]);
    let shown = panel.clone();
    let app = move || ui::app(shown.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    panel.fail(ERR);
    settle(&mut t);
    let (_, at) = paragraph_with(&t, ERR).expect("the error, as selectable text");
    let y = f64::from(at.center().y);
    let (from, to) = (
        (f64::from(at.min_x()) + 2.0, y),
        (f64::from(at.min_x()) + 120.0, y),
    );
    t.move_cursor(from);
    t.press_cursor(from);
    settle(&mut t); // the press focuses the text, which the drag needs
    t.move_cursor(to);
    t.release_cursor(to);
    settle(&mut t);
    let (lit, _) = paragraph_with(&t, ERR).expect("still there");
    assert!(!lit.is_empty(), "the drag selected part of the error");
}
