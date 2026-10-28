//! The Settings form's hotkey fields: Record takes the next key by its name.

use super::kit::*;
use blitznir_app::ui;
use freya::prelude::{Area, LaunchProxy, Size2D};
use freya_testing::prelude::*;

/// The first Record button on the Hotkeys page.
fn record_button(t: &mut TestingRunner) -> Area {
    press_label(t, "Hotkeys");
    for _ in 0..40 {
        let on_screen = t
            .find_many(|node, e| {
                Label::try_downcast(e)
                    .filter(|l| l.text.as_ref() == "Record")
                    .map(|_| node.layout().area)
            })
            .into_iter()
            .find(|a| a.min_y() > 40.0 && a.max_y() < WINDOW.1 - 60.0);
        if let Some(a) = on_screen {
            return a;
        }
        t.scroll((500.0, 300.0), (0.0, -60.0));
        settle(t);
    }
    panic!("no Record button on screen");
}

/// Record, then keys from the hotkeys' listener (as the OS tap reports
/// them): a letter is refused, left Ctrl goes in by its name; the field
/// types nothing meanwhile.
pub fn record_takes_the_pressed_keys_name(proxy: &LaunchProxy) {
    use blitznir_core::keys::Key as K;
    use blitznir_core::platform::KeyEvent;
    let panel = panel(proxy, "record", &[]);
    panel.show_settings(true);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    let at = record_button(&mut t);
    let centre = (f64::from(at.center().x), f64::from(at.center().y));
    t.move_cursor(centre);
    t.click_cursor(centre);
    settle(&mut t);
    assert!(panel.ui().recording.is_some(), "Record waits for a key");
    let before = panel.ui().config.text.text;
    panel.saw(KeyEvent::Other); // a letter
    settle(&mut t);
    shot(&mut t, "record-refused");
    assert_eq!(
        panel.ui().recording.map(|r| r.1),
        Some(true),
        "refused, still waiting"
    );
    assert_eq!(panel.ui().config.text.text, before, "nothing taken");
    let down = KeyEvent::Modifier {
        key: K::LeftCtrl,
        down: true,
    };
    panel.saw(down);
    settle(&mut t);
    shot(&mut t, "record-done");
    assert!(panel.ui().recording.is_none(), "done");
    let text = panel.ui().config.text.text;
    assert!(
        text.contains("\"LeftCtrl\""),
        "config.toml's text has it:\n{text}"
    );
}

/// A form change is saved by itself once the changes rest: config.toml has
/// it without Save, though another field's input was refused before it.
pub fn the_form_saves_itself(proxy: &LaunchProxy) {
    use blitznir_app::panel::form::Target;
    use blitznir_core::config::form::field;
    let panel = panel(proxy, "autosave", &[]);
    panel.show_settings(true);
    let tap = field("hotkeys", "double_tap_ms").expect("in the form");
    panel.form_set(Target::Field(tap), "6".into()); // refused: 100 to 2000
    assert!(panel.ui().form_error.is_some(), "refused");
    let f = field("dictation", "rewrite_in_panel").expect("in the form");
    panel.form_set(Target::Field(f), "true".into());
    let file = std::env::temp_dir()
        .join(format!("blitznir-ui-autosave-{}", std::process::id()))
        .join("config.toml");
    let read = || std::fs::read_to_string(&file).unwrap_or_default();
    wait_for("the autosave", || {
        read().contains("rewrite_in_panel = true")
    });
    assert!(!panel.ui().config.dirty, "nothing left unsaved");
}

/// A job that starts while a form change waits to be saved holds the save
/// back, never drops it: it is written once the job is over. Reset to
/// defaults meanwhile is no form change: it waits for Save.
pub fn a_job_delays_the_autosave(proxy: &LaunchProxy) {
    use blitznir_app::panel::form::Target;
    use blitznir_core::config::form::field;
    use std::sync::Arc;
    let cfg = local_config(&silent_endpoint());
    let platform = Arc::new(blitznir_core::platform::fake::FakePlatform::new());
    let panel = panel_over(proxy, "autosave-busy", &[], platform, cfg);
    panel.show_settings(true);
    let f = field("dictation", "rewrite_in_panel").expect("in the form");
    panel.form_set(Target::Field(f), "true".into());
    panel.rewrite_dictation("hello there".into(), true); // runs until Esc
    wait_for("the rewrite to run", || panel.ui().busy.is_some());
    std::thread::sleep(std::time::Duration::from_millis(1000)); // past the autosave's wait
    panel.escape();
    let file = std::env::temp_dir()
        .join(format!("blitznir-ui-autosave-busy-{}", std::process::id()))
        .join("config.toml");
    let saved =
        || std::fs::read_to_string(&file).is_ok_and(|t| t.contains("rewrite_in_panel = true"));
    wait_for("the autosave after the job", saved);
    let tap = field("hotkeys", "double_tap_ms").expect("in the form");
    panel.form_set(Target::Field(tap), "500".into());
    panel.reset_config();
    std::thread::sleep(std::time::Duration::from_millis(1200));
    assert!(saved(), "the defaults wait for Save");
    assert!(panel.ui().config.dirty);
}

/// The search at the top of the sidebar narrows the form to what matches.
pub fn settings_search_narrows_the_form(proxy: &LaunchProxy) {
    let panel = panel(proxy, "settings-search", &[]);
    panel.show_settings(true);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    let (_, at) = paragraph_with(&t, "Search settings").expect("the search field");
    let centre = (f64::from(at.center().x), f64::from(at.center().y));
    t.move_cursor(centre);
    t.click_cursor(centre);
    settle(&mut t);
    t.write_text("quick translate");
    settle(&mut t);
    shot(&mut t, "settings-search");
    assert!(
        paragraph_with(&t, "Quick translate").is_some(),
        "the match stays"
    );
    assert!(
        paragraph_with(&t, "Live dictation").is_none(),
        "the rest goes"
    );
    t.write_text(" zzz");
    settle(&mut t);
    let none =
        t.find(|_, e| Label::try_downcast(e).filter(|l| l.text.starts_with("No setting matches")));
    assert!(none.is_some(), "says when nothing matches");
}

/// The main sidebar: in a short window a saved chat still shows; History
/// waits folded below the chats until pressed; a press on a list's heading
/// folds it away; both kept in state.json.
pub fn the_sidebar_keeps_the_chats_and_folds_lists(proxy: &LaunchProxy) {
    let chat = long_chat();
    let panel = panel(proxy, "sidebar", std::slice::from_ref(&chat));
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, 300.0), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "sidebar-short");
    let row = t.find(|node, e| {
        Label::try_downcast(e)
            .filter(|l| l.text.as_ref() == chat.name)
            .map(|_| node.layout().area)
    });
    let row = row.or_else(|| paragraph_with(&t, &chat.name).map(|(_, a)| a));
    let row = row.expect("the chat's row");
    assert!(
        row.max_y() <= 300.0,
        "the chat shows in a 300 pt window: {row:?}"
    );
    let label_at = |t: &TestingRunner, start: &str| {
        t.find(|node, e| {
            Label::try_downcast(e)
                .filter(|l| l.text.starts_with(start))
                .map(|_| node.layout().area)
        })
    };
    // History, for finding something again, starts folded at the bottom
    assert!(
        label_at(&t, "Dictations ·").is_none(),
        "History starts folded"
    );
    let history = label_at(&t, "History ·").expect("the History heading");
    assert!(history.min_y() > row.min_y(), "below the chats");
    press_label_starting(&mut t, "History ·");
    assert!(panel.ui().history_open, "unfolded");
    let state = std::env::temp_dir()
        .join(format!("blitznir-ui-sidebar-{}", std::process::id()))
        .join("state.json");
    let saved = || std::fs::read_to_string(&state).unwrap_or_default();
    assert!(
        saved().contains("\"history_open\": true"),
        "kept: {}",
        saved()
    );
    let head = label_at(&t, "Dictations ·").expect("the Dictations heading");
    let at = (f64::from(head.center().x), f64::from(head.center().y));
    t.move_cursor(at);
    t.click_cursor(at);
    settle(&mut t);
    assert_eq!(panel.ui().closed, ["dictations"], "folded");
    assert!(
        saved().contains("dictations"),
        "kept in state.json: {}",
        saved()
    );
}

/// Seeded: 10 dictations and 10 rewrites, a 300 pt window: the Chats heading
/// stays on screen with the chats shown and with them folded.
pub fn the_chats_heading_stays_in_a_short_full_window(proxy: &LaunchProxy) {
    use blitznir_core::platform::fake::FakePlatform;
    use blitznir_core::store::Made;
    let chat = long_chat();
    let seed = |s: &blitznir_core::store::Store| {
        for i in 0..10 {
            s.add_history(&format!("dictated line {i}"), "live")
                .expect("a dictation");
            let made = Made::new("rewrite", "Rewrite", None);
            s.add_replacement(&format!("Rewritten {i}."), &format!("said {i}"), &made)
                .expect("a rewrite");
        }
    };
    let platform = std::sync::Arc::new(FakePlatform::new());
    let cfg = blitznir_core::config::Config::default();
    let panel = panel_seeded(
        proxy,
        "sidebar-full",
        std::slice::from_ref(&chat),
        platform,
        cfg,
        seed,
    );
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, 300.0), |_| {}, 1.0);
    settle(&mut t);
    let heading = |t: &TestingRunner| {
        t.find(|node, e| {
            Label::try_downcast(e)
                .filter(|l| l.text.starts_with("Chats ·"))
                .map(|_| node.layout().area)
        })
        .expect("the Chats heading")
    };
    shot(&mut t, "sidebar-full-open");
    assert!(heading(&t).max_y() <= 300.0, "open: {:?}", heading(&t));
    let at = heading(&t);
    let at = (f64::from(at.center().x), f64::from(at.center().y));
    t.move_cursor(at);
    t.click_cursor(at);
    settle(&mut t);
    shot(&mut t, "sidebar-full-folded");
    assert_eq!(panel.ui().closed, ["chats"]);
    assert!(heading(&t).max_y() <= 300.0, "folded: {:?}", heading(&t));
}

/// Settings' About: a page of its own from the sidebar, not the form's last
/// card; the version, the data folder and the credits.
pub fn settings_about_names_the_build_and_credits(proxy: &LaunchProxy) {
    let panel = panel(proxy, "about", &[]);
    panel.show_settings(true);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    t.scroll((400.0, 300.0), (0.0, -100_000.0));
    settle(&mut t);
    assert!(
        paragraph_with(&t, "Blitznir 0.").is_none(),
        "the form's bottom is no About"
    );
    press_label(&mut t, "About");
    shot(&mut t, "settings-about");
    let save = t.find(|_, e| Label::try_downcast(e).filter(|l| l.text.as_ref() == "Save"));
    assert!(save.is_none(), "nothing to save on About");
    assert!(paragraph_with(&t, "Blitznir 0.").is_some(), "the version");
    assert!(
        paragraph_with(&t, "Parakeet TDT").is_some(),
        "the model's credit"
    );
    assert!(
        paragraph_with(&t, "blitznir-ui-about").is_some(),
        "the data folder"
    );
    // pressed only by hand: the tests send nothing to GitHub
    let check =
        t.find(|_, e| Label::try_downcast(e).filter(|l| l.text.as_ref() == "Check for updates"));
    assert!(check.is_some(), "the update button");
}

/// A drag in a chat, then a scroll with the button still down and the
/// pointer still (a trackpad scroll moves no pointer): the selection reaches
/// the text now under the pointer, not where it was before the scroll.
pub fn a_scroll_mid_drag_extends_the_selection(proxy: &LaunchProxy) {
    let chat = long_chat();
    let panel = panel(proxy, "drag-scroll", std::slice::from_ref(&chat));
    panel.open_chat(&chat.id);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    t.scroll((500.0, 250.0), (0.0, 4000.0)); // to the chat's top
    settle(&mut t);
    // how much is selected: the highlighted characters over every paragraph
    let lit = |t: &TestingRunner| -> usize {
        let all = t.find_many(|_, e| {
            Paragraph::try_downcast(e)
                .map(|p| p.highlights.iter().map(|(a, b)| b - a).sum::<usize>())
        });
        all.into_iter().sum()
    };
    t.move_cursor((300.0, 120.0));
    t.press_cursor((300.0, 120.0));
    t.move_cursor((300.0, 300.0));
    settle(&mut t);
    let before = lit(&t);
    shot(&mut t, "drag-before");
    // the wheel alone, as a trackpad sends it: no pointer move after it
    t.send_event(PlatformEvent::Wheel {
        name: WheelEventName::Wheel,
        scroll: (0.0, -200.0).into(),
        cursor: (300.0, 300.0).into(),
        source: WheelSource::Pixel,
        phase: TouchPhase::Moved,
    });
    settle(&mut t);
    let after = lit(&t);
    shot(&mut t, "drag-after");
    t.release_cursor((300.0, 300.0));
    assert!(before > 0, "the drag selected something");
    assert!(
        after > before,
        "the scroll took the selection on: {before} -> {after}"
    );
}
