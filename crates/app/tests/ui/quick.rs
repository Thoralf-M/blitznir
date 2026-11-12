//! `[hotkeys] translate`: the quick translation, in place or in the panel.

use super::kit::*;
use blitznir_app::ui;
use blitznir_core::{
    lock,
    platform::{KeyEvent, Platform, fake::FakePlatform},
    tasks::Kind,
};
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

/// `[translate] on_select`: a mouse selection gets the Translate button,
/// whose click brings the app back and translates in place; off, or with
/// nothing selected, or off macOS (no place for it), no button.
pub fn a_mouse_selection_offers_translate(proxy: &LaunchProxy) {
    let wait = || std::thread::sleep(std::time::Duration::from_millis(400));
    let mac = cfg!(target_os = "macos");
    let mut cases = vec![("pick-off", false, "Hello"), ("pick-none", true, " ")];
    if !mac {
        cases.push(("pick-linux", true, "Hello"));
    }
    for (name, on, text) in cases {
        let mut fake = FakePlatform::new();
        fake.mouse = Some((400.0, 400.0));
        *lock(&fake.selection) = Some(text.into());
        let platform = Arc::new(fake);
        let mut cfg = local_config(&answering_endpoint("Hallo"));
        cfg.translate.on_select = on;
        let panel = panel_over(proxy, name, &[], platform.clone(), cfg);
        panel.saw(KeyEvent::Selected);
        wait();
        panel.translate_picked(); // a no-op: no button
        wait();
        assert!(lock(&platform.refocused).is_empty(), "{name}: no button");
    }
    if !mac {
        return;
    }
    let mut fake = FakePlatform::new();
    fake.mouse = Some((400.0, 400.0));
    *lock(&fake.selection) = Some("Hello world".into());
    let platform = Arc::new(fake);
    let mut cfg = local_config(&answering_endpoint("Hallo Welt"));
    cfg.translate.on_select = true;
    let panel = panel_over(proxy, "pick", &[], platform.clone(), cfg);
    panel.saw(KeyEvent::Selected);
    assert!(platform.clipboard().is_empty(), "looked at, not copied");
    wait_for("the button's translation", || {
        panel.translate_picked(); // a no-op until the button is up
        platform.screen() == "Hallo Welt"
    });
    assert_eq!(
        lock(&platform.refocused).len(),
        1,
        "the app came back first"
    );
    assert!(!panel.ui().shown, "no panel");
    let no_button = |why: &str| {
        wait();
        panel.translate_picked();
        wait();
        assert_eq!(lock(&platform.refocused).len(), 1, "{why}: no button");
    };
    // the same selection again: a window moved, a scrollbar was dragged
    panel.saw(KeyEvent::Selected);
    no_button("nothing newly selected");
    // typed on before the button could show: it would come too late
    *lock(&platform.selection) = Some("Good night".into());
    panel.saw(KeyEvent::Selected);
    panel.saw(KeyEvent::Other);
    no_button("a key right after");
    // only a shown button is remembered: selected again, it shows now
    panel.saw(KeyEvent::Selected);
    wait_for("the button for the text the key cut short", || {
        panel.translate_picked();
        lock(&platform.refocused).len() == 2
    });
    // (a job running gets no button: the last one first)
    wait_for("its paste", || lock(&platform.pasted_into).len() == 2);
    wait_for("its job done", || panel.ui().busy.is_none());
    // the app dropped the selection while the button had the click: the
    // text read at the button is translated and shown, not pasted
    *lock(&platform.selection) = Some("Good morning".into());
    panel.saw(KeyEvent::Selected);
    // the box asks the event loop for the screens first: 1 s without one here
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let pastes = lock(&platform.pasted_into).len();
    *lock(&platform.selection) = None;
    panel.translate_picked();
    wait_for("the translation shown", || {
        let ui = panel.ui();
        ui.selection.as_deref() == Some("Good morning")
            && ui.busy.is_none()
            && !ui.messages.is_empty()
    });
    assert_eq!(lock(&platform.pasted_into).len(), pastes, "nothing pasted");
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
    // a translation is the history's, not a new chat
    assert!(ui.chats.is_empty(), "no chat saved");
    assert_eq!(
        ui.recent.first().map(|r| r.mode.as_str()),
        Some("translate")
    );
    let app = move || ui::app(panel.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "quick-translate-shown");
}

/// The same on a page, with somewhere to put the box (the mouse): the
/// translation shows in the box by the text, not the panel; Open in Blitznir
/// then shows the panel on its chat.
pub fn a_quick_translation_on_a_page_shows_the_box(proxy: &LaunchProxy) {
    if !cfg!(target_os = "macos") {
        return; // the box by the text is macOS's
    }
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
