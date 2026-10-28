//! `[hotkeys] translate`: the quick translation, in place or in the panel.

use super::kit::*;
use blitznir_app::ui;
use blitznir_core::lock;
use blitznir_core::platform::fake::FakePlatform;
use blitznir_core::platform::{KeyEvent, Platform};
use blitznir_core::tasks::Kind;
use freya::prelude::{LaunchProxy, Size2D};
use freya_testing::prelude::*;
use std::sync::Arc;

/// A Fake app with `selected` selected, taking text (`editable`) or not.
fn selected(selected: &str, editable: Option<bool>) -> Arc<FakePlatform> {
    let mut platform = FakePlatform::new();
    platform.editable = editable;
    *lock(&platform.selection) = Some(selected.into());
    Arc::new(platform)
}

/// The selection replaced by its translation, no panel.
pub fn a_quick_translation_replaces_the_selection(proxy: &LaunchProxy) {
    let platform = selected("Hello world", None);
    let cfg = local_config(&answering_endpoint("Hallo Welt"));
    let panel = panel_over(proxy, "quick-translate", &[], platform.clone(), cfg);
    panel.quick(Kind::Translate, None);
    let pasted = || !lock(&platform.pasted_into).is_empty();
    wait_for("the paste", pasted);
    assert_eq!(platform.screen(), "Hallo Welt");
    wait_for("the history", || !panel.ui().recent.is_empty());
    let ui = panel.ui();
    assert!(!ui.shown, "no panel");
    let kept = &ui.recent[0];
    assert_eq!(
        (kept.mode.as_str(), kept.action.as_deref()),
        ("translate", Some("Translate"))
    );
}

/// On a page that takes no text the quick translation is shown in the panel.
pub fn a_quick_translation_on_a_page_shows_the_panel(proxy: &LaunchProxy) {
    let platform = selected("Hello world", Some(false));
    let cfg = local_config(&answering_endpoint("Hallo Welt"));
    let panel = panel_over(proxy, "quick-shown", &[], platform.clone(), cfg);
    panel.quick(Kind::Translate, None);
    wait_for("the panel", || panel.ui().shown);
    let ui = panel.ui();
    assert!(ui.chatting && lock(&platform.pasted_into).is_empty());
    assert_eq!(
        ui.messages.last().map(|m| m.text.as_str()),
        Some("Hallo Welt")
    );
    assert_eq!(platform.clipboard(), "Hallo Welt");
    let app = move || ui::app(panel.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "quick-translate-shown");
}

/// The same on a page, with somewhere to put the box (the mouse): the
/// translation shows in the box by the text, not the panel; Open in Blitznir
/// then shows the panel on its chat.
pub fn a_quick_translation_on_a_page_shows_the_box(proxy: &LaunchProxy) {
    let mut fake = FakePlatform::new();
    fake.editable = Some(false);
    fake.mouse = Some((400.0, 400.0));
    *lock(&fake.selection) = Some("Hello world".into());
    let platform = Arc::new(fake);
    let cfg = local_config(&answering_endpoint("Hallo Welt"));
    let panel = panel_over(proxy, "quick-box", &[], platform.clone(), cfg);
    panel.quick(Kind::Translate, None);
    let translated = || {
        panel
            .ui()
            .messages
            .last()
            .is_some_and(|m| m.text == "Hallo Welt")
    };
    wait_for("the translation", translated);
    // the box asks the event loop for the screens first: 1 s without one here
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert!(!panel.ui().shown, "the box, not the panel");
    assert!(lock(&platform.pasted_into).is_empty(), "nothing pasted");
    // a screenshot of it: the shortcut and the region's drag keep the box
    panel.saw(KeyEvent::Screenshot);
    panel.saw(KeyEvent::Click);
    platform.set_clipboard("before").expect("the clipboard");
    panel.copy_offer();
    assert_eq!(
        platform.clipboard(),
        "Hallo Welt",
        "still up: its Copy copied"
    );
    panel.open_offer();
    wait_for("the panel on its chat", || {
        panel.ui().shown && panel.ui().chatting
    });
}
