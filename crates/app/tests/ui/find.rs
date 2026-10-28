//! Settings' config.toml tab: the sidebar keeps its search, which finds in
//! the file, and About, but lists no sections.

use super::kit::*;
use blitznir_app::ui;
use freya::prelude::{LaunchProxy, Size2D};
use freya_testing::prelude::*;

/// The sidebar's line saying what the search found, if any.
fn found(t: &TestingRunner) -> Option<String> {
    t.find(|_, e| {
        Label::try_downcast(e)
            .map(|l| l.text.to_string())
            .filter(|s| s.contains(" of ") || s.starts_with("Not in"))
    })
}

fn has_label(t: &TestingRunner, text: &str) -> bool {
    label_at(t, text).is_some()
}

pub fn the_config_tab_keeps_the_search_and_about(proxy: &LaunchProxy) {
    let panel = panel(proxy, "find", &[]);
    panel.show_settings(true);
    let app = move || ui::app(panel.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    press_label(&mut t, "config.toml");
    assert!(!has_label(&t, "Hotkeys"), "no sections beside the text");
    assert!(has_label(&t, "About"), "About stays");
    let (_, at) = paragraph_with(&t, "Search settings").expect("the search stays");
    let centre = (f64::from(at.center().x), f64::from(at.center().y));
    t.move_cursor(centre);
    t.click_cursor(centre);
    t.write_text("effect");
    settle(&mut t);
    shot(&mut t, "config-find");
    let first = found(&t).expect("what the search found");
    assert!(first.starts_with("1 of "), "the first match: {first}");
    t.press_key(Key::Named(NamedKey::Enter));
    settle(&mut t);
    let second = found(&t).expect("still found");
    assert!(second.starts_with("2 of "), "Return, the next: {second}");
    t.write_text("zzzq");
    settle(&mut t);
    let none = found(&t).expect("says so");
    assert!(none.starts_with("Not in"), "no match: {none}");
    press_label(&mut t, "About");
    assert!(
        paragraph_with(&t, "Blitznir 0.").is_some(),
        "About from the text tab"
    );
    press_label(&mut t, "config.toml");
    assert!(
        paragraph_with(&t, "Blitznir 0.").is_none(),
        "and back to the text"
    );
}
