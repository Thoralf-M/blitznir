//! Settings as pages: the sidebar shows one section at a time, its rarely
//! changed settings folded under Advanced; a search finds across pages and
//! unfolds; ⌘F reaches the search from inside config.toml's editor.

use super::kit::*;
use blitznir_app::ui;
use freya::prelude::{LaunchProxy, Size2D};
use freya_testing::prelude::*;

fn settings(
    proxy: &LaunchProxy,
    name: &str,
) -> (TestingRunner, std::sync::Arc<blitznir_app::panel::Panel>) {
    let panel = panel(proxy, name, &[]);
    panel.show_settings(true);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, 900.0), |_| {}, 1.0);
    settle(&mut t);
    (t, panel)
}

/// A press on Effects shows Effects alone (the last pages were never
/// reached by scrolling the one long form); Dictation's rare settings wait
/// under Advanced; a search shows them without unfolding.
pub fn settings_are_pages_with_advanced_folded(proxy: &LaunchProxy) {
    let (mut t, _) = settings(proxy, "pages");
    press_label(&mut t, "Effects");
    assert!(
        paragraph_with(&t, "Wave while a chat answers").is_some(),
        "Effects shows"
    );
    assert!(
        paragraph_with(&t, "Speech model").is_none(),
        "and nothing else"
    );
    press_label(&mut t, "Dictation");
    assert!(
        paragraph_with(&t, "Speech model").is_some(),
        "Dictation shows"
    );
    assert!(
        paragraph_with(&t, "Silence level").is_none(),
        "folded under Advanced"
    );
    press_label(&mut t, "Advanced (3)");
    assert!(paragraph_with(&t, "Silence level").is_some(), "unfolded");
    press_label(&mut t, "Effects");
    press_label(&mut t, "Dictation");
    assert!(
        paragraph_with(&t, "Silence level").is_none(),
        "folded again on another visit"
    );
    let (_, at) = paragraph_with(&t, "Search settings").expect("the search");
    let c = (f64::from(at.center().x), f64::from(at.center().y));
    t.move_cursor(c);
    t.click_cursor(c);
    t.write_text("silence");
    settle(&mut t);
    assert!(
        paragraph_with(&t, "Silence level").is_some(),
        "found, not folded"
    );
}

/// ⌘F inside config.toml's editor goes to the sidebar's search: the editor
/// types no "f", and what is typed next searches.
pub fn command_f_in_the_editor_searches(proxy: &LaunchProxy) {
    let (mut t, panel) = settings(proxy, "cmd-f");
    press_label(&mut t, "config.toml");
    let (_, at) = paragraph_with(&t, "double_tap_ms").expect("the editor's text");
    let c = (f64::from(at.min_x()) + 4.0, f64::from(at.center().y));
    t.move_cursor(c);
    t.click_cursor(c);
    settle(&mut t);
    let before = panel.ui().config.text.text;
    let command = if cfg!(target_os = "macos") {
        Modifiers::META
    } else {
        Modifiers::CONTROL
    };
    t.send_event(PlatformEvent::Keyboard {
        name: KeyboardEventName::KeyDown,
        key: Key::Character("f".into()),
        code: Code::KeyF,
        modifiers: command,
    });
    settle(&mut t);
    t.write_text("effect");
    settle(&mut t);
    assert_eq!(
        panel.ui().config.text.text,
        before,
        "the editor typed nothing"
    );
    let found = label_where(&t, |l| l.starts_with("1 of "));
    assert!(found.is_some(), "the search has it");
}

/// Enough chats to scroll: each one's message count stays left of the
/// scrollbar, which lies over the rows' right end.
pub fn the_chat_counts_clear_the_scrollbar(proxy: &LaunchProxy) {
    let chats: Vec<_> = (0..30)
        .map(|i| {
            let mut c = long_chat();
            c.id = format!("chat-{i}");
            c.name = format!("Chat number {i}");
            c
        })
        .collect();
    let panel = panel(proxy, "bar", &chats);
    let app = move || ui::app(panel.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, 400.0), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "chats-scrolling");
    let counts = t.find_many(|node, e| {
        Label::try_downcast(e)
            .filter(|l| l.text.as_ref() == "2")
            .map(|_| node.layout().area)
    });
    assert!(!counts.is_empty(), "the counts");
    // the sidebar's right edge, less its padding and the bar's 14 pt
    let room = blitznir_core::store::State::SIDE_DEFAULT - 6.0 - 14.0;
    for a in counts {
        assert!(a.max_x() <= room + 0.5, "a count under the bar: {a:?}");
    }
}

/// ⌘F with the sidebar away, folded or (a narrow window) not peeking: the
/// sidebar shows and what is typed next searches, not the composer.
pub fn command_f_shows_the_sidebar_and_searches(proxy: &LaunchProxy) {
    for (width, folded) in [(WINDOW.0, true), (500.0, false)] {
        let panel = panel(proxy, &format!("cmd-f-{width}"), &[long_chat()]);
        panel.set_folded(folded);
        let p = panel.clone();
        let app = move || ui::app(p.clone());
        let (mut t, ()) = TestingRunner::new(app, Size2D::new(width, 400.0), |_| {}, 1.0);
        settle(&mut t);
        let command = if cfg!(target_os = "macos") {
            Modifiers::META
        } else {
            Modifiers::CONTROL
        };
        t.send_event(PlatformEvent::Keyboard {
            name: KeyboardEventName::KeyDown,
            key: Key::Character("f".into()),
            code: Code::KeyF,
            modifiers: command,
        });
        animate(&mut t, 200);
        t.write_text("zzz");
        settle(&mut t);
        let none = label_at(&t, "No matches");
        assert!(
            none.is_some(),
            "{width} pt, folded {folded}: the search took it"
        );
    }
}

/// A search that finds a dictation holds History and its lists open: a
/// press on their headings then folds nothing, here or in state.json.
pub fn a_search_holds_the_history_open(proxy: &LaunchProxy) {
    use blitznir_core::platform::fake::FakePlatform;
    let seed = |s: &blitznir_core::store::Store| {
        s.add_history("the quarterly numbers", "live")
            .expect("a dictation");
    };
    let platform = std::sync::Arc::new(FakePlatform::new());
    let cfg = blitznir_core::config::Config::default();
    let panel = panel_seeded(proxy, "held", &[], platform, cfg, seed);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    let (_, at) = paragraph_with(&t, "Search chats and history").expect("the search");
    let c = (f64::from(at.center().x), f64::from(at.center().y));
    t.move_cursor(c);
    t.click_cursor(c);
    t.write_text("quarterly");
    settle(&mut t);
    assert!(
        paragraph_with(&t, "the quarterly numbers").is_some(),
        "found, History open"
    );
    press_label_starting(&mut t, "History ·");
    press_label_starting(&mut t, "Dictations ·");
    let ui = panel.ui();
    assert!(
        !ui.history_open && ui.closed.is_empty(),
        "nothing folded or kept: {:?}",
        ui.closed
    );
    assert!(
        paragraph_with(&t, "the quarterly numbers").is_some(),
        "still shown"
    );
}

/// Record left on as Settings is left (Esc) or the panel closed: a modifier
/// pressed later, in another app, is no hotkey's new value.
pub fn a_record_ends_with_settings(proxy: &LaunchProxy) {
    use blitznir_app::panel::form::Target;
    use blitznir_core::config::form::{field, value};
    use blitznir_core::keys::Key as K;
    use blitznir_core::platform::KeyEvent;
    let panel = panel(proxy, "record-left", &[]);
    let chat = field("hotkeys", "chat").expect("in the form");
    let cmd = KeyEvent::Modifier {
        key: K::LeftMeta,
        down: true,
    };
    let after = |leave: &dyn Fn()| {
        panel.show_settings(true);
        panel.record(Some(Target::Field(chat)));
        leave();
        panel.saw(cmd); // in another app
        value(&panel.ui().config.text.text, chat)
    };
    let esc = after(&|| {
        panel.saw(KeyEvent::Other); // the Esc, as the hotkeys' listener sees it
        panel.escape(); // and as the window does: Settings left
    });
    assert_eq!(esc, "", "nothing recorded after Esc");
    assert_eq!(after(&|| panel.close()), "", "nor after a close");
    assert!(panel.ui().recording.is_none(), "the record ended");
}

/// The wheel in Settings: in the margin right of the fields it scrolls
/// the page; over the sidebar it turns the pages, at most one per 120 ms (a
/// scroll goes on page by page, momentum soon stops); in a window too short for the
/// list of pages it scrolls that list instead, so About stays reachable.
pub fn settings_scroll_from_the_side_and_the_margins(proxy: &LaunchProxy) {
    let panel = panel(proxy, "settings-wheel", &[]);
    panel.show_settings(true);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    press_label(&mut t, "Dictation"); // a page taller than the window
    let y = |t: &TestingRunner| {
        paragraph_with(t, "Speech model")
            .expect("in the form")
            .1
            .min_y()
    };
    let start = y(&t);
    t.scroll((WINDOW.0 as f64 - 6.0, 250.0), (0.0, -120.0)); // right of the fields
    settle(&mut t);
    let margin = y(&t);
    assert!(
        margin < start - 60.0,
        "the margin scrolls the page: {start} -> {margin}"
    );
    t.scroll((100.0, 250.0), (0.0, -50.0)); // the sidebar, down
    t.scroll((100.0, 250.0), (0.0, -50.0)); // too soon after the turn: dropped
    settle(&mut t);
    assert!(
        paragraph_with(&t, "Keep as written").is_some(),
        "one page on: Vocabulary"
    );
    std::thread::sleep(std::time::Duration::from_millis(200));
    t.scroll((100.0, 250.0), (0.0, 50.0)); // and up
    settle(&mut t);
    assert!(paragraph_with(&t, "Speech model").is_some(), "back");
    // a scroll that goes on, a step every 30 ms, goes on page by page
    for _ in 0..12 {
        t.scroll((100.0, 250.0), (0.0, -20.0));
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    settle(&mut t);
    let on = paragraph_with(&t, "Speech model").is_none()
        && paragraph_with(&t, "Keep as written").is_none();
    assert!(on, "past Vocabulary: more than one page");
    // a window too short for the list: the wheel scrolls it to About
    let short = panel.clone();
    let app = move || ui::app(short.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, 320.0), |_| {}, 1.0);
    settle(&mut t);
    t.scroll((100.0, 250.0), (0.0, -400.0));
    settle(&mut t);
    let about = t
        .find(|node, e| {
            Label::try_downcast(e)
                .filter(|l| l.text.as_ref() == "About")
                .map(|_| node.layout().area)
        })
        .expect("About");
    assert!(
        about.max_y() <= 320.0,
        "About scrolled into the window: {about:?}"
    );
}
