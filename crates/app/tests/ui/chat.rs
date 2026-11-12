//! A chat's answers, as the model wrote them.

use super::kit::*;
use blitznir_app::{panel::Mode, ui};
use blitznir_core::store::Chat;
use freya::prelude::{Area, LaunchProxy, Size2D};
use freya_testing::prelude::*;

/// An answer that opens with a code block keeps its fences: markdown to
/// draw, not a rewrite to unwrap (its code came out as text, and the
/// explanation after it as code).
pub fn a_chat_answer_keeps_its_code_fences(proxy: &LaunchProxy) {
    // JSON-escaped: the endpoint sends it as written
    let url = answering_endpoint("```py\\nx = 1\\n```\\nThat sets x.");
    let panel = panel_over(proxy, "chat-fences", &[], fake(), local_config(&url));
    panel.open(Mode::Chat, Some(String::new()));
    wait_for("the open", || {
        let ui = panel.ui();
        ui.chatting && ui.selection.is_some()
    });
    panel.set_composer("Set x in Python".into());
    panel.send();
    wait_for("the answer", || {
        let ui = panel.ui();
        ui.messages.len() == 2 && ui.busy.is_none()
    });
    let ui = panel.ui();
    assert_eq!(ui.messages[1].text, "```py\nx = 1\n```\nThat sets x.");
}

/// A chat named `name` with one exchange.
fn named(name: &str) -> Chat {
    let mut chat = Chat::new();
    chat.name = name.into();
    chat.messages = vec![
        message("user", "What is left to do?".into()),
        message("assistant", "Noted.".into()),
    ];
    chat
}

/// The paragraph holding `text` right of the sidebar: the open chat's head.
fn in_head(t: &TestingRunner, text: &str) -> Option<Area> {
    let side = blitznir_core::store::State::SIDE_DEFAULT;
    t.find_many(|node, e| {
        Paragraph::try_downcast(e)
            .filter(|p| p.spans.iter().any(|s| s.text.contains(text)))
            .map(|_| node.layout().area)
    })
    .into_iter()
    .find(|a| a.min_x() > side)
}

/// The pencil opened on one chat's title, then another chat opened: its
/// head shows its own title, not the first one's name in a field whose
/// Return would rename it.
pub fn a_rename_stays_with_its_chat(proxy: &LaunchProxy) {
    let (a, b) = (named("Alpha plans"), named("Beta notes"));
    let panel = panel(proxy, "rename-stays", &[a.clone(), b.clone()]);
    panel.open_chat(&a.id);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    let title = in_head(&t, "Alpha plans").expect("A's title in its head");
    let pencil = (f64::from(title.max_x()) + 12.0, f64::from(title.center().y));
    t.move_cursor(pencil);
    t.click_cursor(pencil);
    settle(&mut t);
    let save = |t: &TestingRunner| {
        t.find(|_, e| Label::try_downcast(e).filter(|l| l.text.as_ref() == "Save"))
            .is_some()
    };
    assert!(save(&t), "the pencil opened the field");
    shot(&mut t, "rename-open");
    panel.open_chat(&b.id);
    settle(&mut t);
    shot(&mut t, "rename-other");
    assert!(in_head(&t, "Beta notes").is_some(), "B's own title shows");
    assert!(
        in_head(&t, "Alpha plans").is_none() && !save(&t),
        "no field with A's name over B"
    );
}

/// A long title in a narrow window is cut short: the head's note and
/// Delete keep their width (squeezed, each wrapped a letter a line).
pub fn a_long_title_leaves_the_head_its_delete(proxy: &LaunchProxy) {
    let chat = named("A title as long as an auto title gets, sixty characters");
    let panel = panel(proxy, "long-title", std::slice::from_ref(&chat));
    panel.open_chat(&chat.id);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(420.0, 400.0), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "long-title");
    let at = |text: &str| label_at(&t, text).unwrap_or_else(|| panic!("the head's {text:?}"));
    let (delete, note) = (at("Delete"), at("2 messages"));
    assert!(
        delete.width() > 30.0 && delete.max_x() <= 420.0,
        "Delete at {delete:?}"
    );
    assert!(note.height() < 20.0, "the note on one line: {note:?}");
}

/// ⌘ and `key`, as the keyboard sends it.
pub fn command(t: &mut TestingRunner, key: &str) {
    let modifiers = if cfg!(target_os = "macos") {
        Modifiers::META
    } else {
        Modifiers::CONTROL
    };
    let code = if key == "a" { Code::KeyA } else { Code::KeyC };
    t.send_event(PlatformEvent::Keyboard {
        name: KeyboardEventName::KeyDown,
        key: Key::Character(key.into()),
        code,
        modifiers,
    });
    settle(t);
}

/// All of one chat selected, then another opened without a press (from the
/// tray's list, by a hotkey): nothing of it is selected, ⌘C copies nothing.
pub fn a_selection_goes_with_its_chat(proxy: &LaunchProxy) {
    let (a, mut b) = (named("Alpha plans"), named("Beta notes"));
    b.messages[1] = message("assistant", "Unrelated text of another chat.".into());
    let platform = fake();
    let chats = [a.clone(), b.clone()];
    let cfg = blitznir_core::config::Config::default();
    let panel = panel_over(proxy, "select-switch", &chats, platform.clone(), cfg);
    panel.open_chat(&a.id);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    // a press in the transcript gives it the keys; ⌘A selects the chat
    let (_, at) = paragraph_with(&t, "Noted.").expect("A's answer");
    let c = (f64::from(at.min_x()) + 4.0, f64::from(at.center().y));
    t.move_cursor(c);
    t.press_cursor(c);
    settle(&mut t);
    t.release_cursor(c);
    settle(&mut t);
    command(&mut t, "a");
    let lit = |t: &TestingRunner, text| paragraph_with(t, text).expect("shown").0;
    assert!(!lit(&t, "Noted.").is_empty(), "A selected");
    panel.open_chat(&b.id);
    settle(&mut t);
    command(&mut t, "c");
    std::thread::sleep(std::time::Duration::from_millis(300)); // a copy's thread
    assert!(
        lit(&t, "Unrelated text").is_empty(),
        "nothing of B selected"
    );
    assert_eq!(platform.clipboard(), "", "nothing copied");
}
