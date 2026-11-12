//! `[[actions]] hotkey`: a button run on the selection without the panel,
//! and where the Settings form edits its hotkey.

use super::kit::*;
use blitznir_app::ui;
use blitznir_core::config::Config;
use blitznir_core::lock;
use blitznir_core::platform::fake::FakePlatform;
use blitznir_core::tasks::Kind;
use freya::prelude::{LaunchProxy, Size2D};
use freya_testing::prelude::*;
use std::sync::Arc;

/// The second button's hotkey runs the second button, not Return's.
pub fn a_buttons_hotkey_runs_that_button(proxy: &LaunchProxy) {
    let mut platform = FakePlatform::new();
    platform.editable = Some(true);
    *lock(&platform.selection) = Some("a long and winding text".into());
    let platform = Arc::new(platform);
    let two = "[[actions]]\nlabel = \"Fix\"\nprompt = \"Fix:\\n{text}\"\nenter = true\n\
               [[actions]]\nlabel = \"Short\"\nprompt = \"Shorten:\\n{text}\"\nhotkey = [\"F14\"]\n";
    let cfg = Config {
        actions: Config::parse(two).expect("two buttons").actions,
        ..local_config(&answering_endpoint("Short."))
    };
    let panel = panel_over(proxy, "button-key", &[], platform.clone(), cfg);
    panel.quick(Kind::Rewrite, Some(1));
    wait_for("the paste", || !lock(&platform.pasted_into).is_empty());
    assert_eq!(platform.screen(), "Short.");
    wait_for("the history", || !panel.ui().recent.is_empty());
    let kept = panel.ui().recent[0].clone();
    let prompt = kept.prompt.unwrap_or_default();
    assert!(
        prompt.starts_with("Shorten:"),
        "the second button's: {prompt}"
    );
}

/// A button's editor in the Settings form has its hotkey under the prompt.
pub fn the_button_editor_has_the_hotkey(proxy: &LaunchProxy) {
    let panel = panel(proxy, "button-editor", &[]);
    panel.show_settings(true);
    let app = move || ui::app(panel.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, 900.0), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "button-editor-closed");
    assert!(paragraph_with(&t, "Runs this button on the selection").is_none());
    // the Rewrite button's prompt preview, in the Buttons table
    let at =
        label_where(&t, |l| l.starts_with("The text below")).expect("the Rewrite button's prompt");
    let open = (f64::from(at.center().x), f64::from(at.center().y));
    t.move_cursor(open);
    t.click_cursor(open);
    settle(&mut t);
    shot(&mut t, "button-editor");
    assert!(
        paragraph_with(&t, "Runs this button on the selection").is_some(),
        "the hotkey's help"
    );
}

/// The composer's tools in a window narrow enough to squeeze them but wide
/// enough to keep the sidebar: Screenshot, the paste icon and the buttons
/// never overlap; the paste button, two gaps and Translate's padding (48
/// pt) lie between the Screenshot and Translate labels when both show;
/// the empty field is as tall as its placeholder.
pub fn the_composer_tools_never_overlap(proxy: &LaunchProxy) {
    for w in (560..=760).step_by(20) {
        let panel = panel(proxy, &format!("tools-{w}"), &[]);
        let app = move || ui::app(panel.clone());
        let (mut t, ()) = TestingRunner::new(app, Size2D::new(w as f32, WINDOW.1), |_| {}, 1.0);
        settle(&mut t);
        let translate = label_at(&t, "Translate").expect("the Translate button");
        let runs = format!("{} runs", blitznir_app::panel::view::RET); // Enter on Linux
        let (_, hint) = paragraph_with(&t, &runs).expect("the placeholder");
        assert!(
            hint.max_y() <= translate.min_y(),
            "at {w} pt the placeholder is cut: it ends at {}, the buttons start at {}",
            hint.max_y(),
            translate.min_y()
        );
        if let Some(shot_label) = label_at(&t, "Screenshot")
            && (shot_label.min_y()..shot_label.max_y()).contains(&translate.center().y)
        {
            let gap = translate.min_x() - shot_label.max_x();
            if gap < 48.0 {
                shot(&mut t, &format!("tools-{w}"));
            }
            assert!(
                gap >= 48.0,
                "at {w} pt the tools overlap: {gap} pt between the labels"
            );
        }
    }
}

/// The model button's tooltip shows on a hover, and goes once the button
/// opens its list: it stood over the models.
pub fn the_model_tooltip_leaves_the_open_list_alone(proxy: &LaunchProxy) {
    let cfg = local_config("http://127.0.0.1:9/v1");
    let platform = Arc::new(FakePlatform::new());
    let panel = panel_over(proxy, "picker-tip", &[], platform, cfg);
    let app = move || ui::app(panel.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    let tip_shown = |t: &TestingRunner| {
        t.find(|_, e| Label::try_downcast(e).filter(|l| l.text.contains("the model for rewriting")))
            .is_some()
    };
    let at = label_at(&t, "m").expect("the picked model");
    let c = (f64::from(at.center().x), f64::from(at.center().y));
    t.move_cursor(c);
    animate(&mut t, 1200); // the tooltip's delay
    assert!(tip_shown(&t), "hovered: the tooltip");
    t.click_cursor(c);
    settle(&mut t);
    animate(&mut t, 1200);
    shot(&mut t, "picker-open");
    assert!(!tip_shown(&t), "the list open: no tooltip over it");
}

/// Two buttons: Rewrite's prompt opened, then the second's, then typing.
/// What is typed goes into the second button's prompt, not Rewrite's: the
/// prompt editor below the table is one input reused for every button.
pub fn a_prompt_edit_goes_to_the_open_button(proxy: &LaunchProxy) {
    use blitznir_app::panel::form::Target;
    use blitznir_core::config::form::{self, ACTIONS, Rows};
    let panel = panel(proxy, "two-prompts", &[]);
    panel.show_settings(true);
    panel.form_add(Rows::Actions);
    panel.form_set(
        Target::Cell(Rows::Actions, 1, &ACTIONS[1]),
        "Summarize this: {text}".into(),
    );
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, 900.0), |_| {}, 1.0);
    settle(&mut t);
    press_label_starting(&mut t, "The text below"); // Rewrite's
    press_label_starting(&mut t, "Summarize this"); // then the second's
    let (_, at) = paragraph_with(&t, "Summarize this").expect("the second's prompt, open");
    let end = (f64::from(at.center().x), f64::from(at.center().y));
    t.move_cursor(end);
    t.click_cursor(end);
    settle(&mut t);
    t.write_text("XYZ");
    settle(&mut t);
    let rows = form::rows(&panel.ui().config.text.text, Rows::Actions);
    assert!(
        rows[0][1].starts_with("The text below"),
        "Rewrite's kept: {:?}",
        rows[0][1].chars().take(40).collect::<String>()
    );
    assert!(
        rows[1][1].contains("XYZ"),
        "the second's has it: {:?}",
        rows[1][1]
    );
}

/// At the window's narrowest, and the narrowest that shows the sidebar,
/// the Buttons table fits its card: each row's trash is there to press.
pub fn the_buttons_table_fits_a_narrow_window(proxy: &LaunchProxy) {
    use freya::prelude::AccessibilityRole;
    for width in [420.0, 560.0] {
        let panel = panel(proxy, &format!("narrow-table-{width}"), &[]);
        panel.show_settings(true);
        let p = panel.clone();
        let app = move || ui::app(p.clone());
        let (mut t, ()) = TestingRunner::new(app, Size2D::new(width, 540.0), |_| {}, 1.0);
        settle(&mut t);
        shot(&mut t, &format!("narrow-table-{width}"));
        let (_, row) = paragraph_with(&t, "Rewrite").expect("the first button's label");
        let y = row.center().y;
        // the row's tooltipped controls: the preview, the two picks, the trash
        let tipped = t.find_many(|node, e| {
            (e.accessibility().builder.role() == AccessibilityRole::Tooltip)
                .then(|| node.layout().area)
                .filter(|a| a.min_y() <= y && a.max_y() >= y)
        });
        let trash = (tipped.into_iter())
            .max_by(|a, b| a.max_x().total_cmp(&b.max_x()))
            .expect("the row's controls");
        // the table's right edge: Settings' 16 pt, the form's 4, the card's 12
        let edge = width - 32.0;
        assert!(trash.max_x() <= edge, "{width} pt: the trash at {trash:?}");
    }
}

/// Three buttons, the second's prompt open, then the first removed: the
/// editor stays on the second's prompt, not on the third's moved up.
pub fn a_removal_above_keeps_the_open_prompt(proxy: &LaunchProxy) {
    use blitznir_app::panel::form::Target;
    use blitznir_core::config::form::{ACTIONS, Rows};
    use freya::prelude::AccessibilityRole;
    let panel = panel(proxy, "remove-above", &[]);
    panel.show_settings(true);
    for (i, name) in [(1, "Second"), (2, "Third")] {
        panel.form_add(Rows::Actions);
        let cell = |c| Target::Cell(Rows::Actions, i, &ACTIONS[c]);
        panel.form_set(cell(0), name.into());
        panel.form_set(cell(1), format!("{name} prompt: {{text}}"));
    }
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, 900.0), |_| {}, 1.0);
    settle(&mut t);
    press_label_starting(&mut t, "Second prompt");
    assert!(paragraph_with(&t, "Prompt · Second").is_some(), "open");
    let (_, first) = paragraph_with(&t, "Rewrite").expect("the first button's label");
    let y = first.center().y;
    let trash = (t.find_many(|node, e| {
        (e.accessibility().builder.role() == AccessibilityRole::Tooltip)
            .then(|| node.layout().area)
            .filter(|a| a.min_y() <= y && a.max_y() >= y)
    }))
    .into_iter()
    .max_by(|a, b| a.max_x().total_cmp(&b.max_x()))
    .expect("the first row's trash");
    let at = (f64::from(trash.center().x), f64::from(trash.center().y));
    t.move_cursor(at);
    t.click_cursor(at);
    settle(&mut t);
    shot(&mut t, "remove-above");
    assert!(paragraph_with(&t, "Rewrite").is_none(), "removed");
    assert!(
        paragraph_with(&t, "Prompt · Second").is_some()
            && paragraph_with(&t, "Prompt · Third").is_none(),
        "the editor stays on the second button"
    );
}

/// A button whose long label wrapped the composer's row onto two, then
/// renamed short: the row is one again, Screenshot with its label.
pub fn the_buttons_row_unwraps_once_it_fits(proxy: &LaunchProxy) {
    let one = |label: &str| {
        format!("[[actions]]\nlabel = \"{label}\"\nprompt = \"Fix:\\n{{text}}\"\nenter = true\n")
    };
    let long = one("A rather long label xyz");
    let cfg = Config {
        actions: Config::parse(&long).expect("a button").actions,
        ..Config::default()
    };
    let panel = panel_over(proxy, "unwrap", &[], Arc::new(FakePlatform::new()), cfg);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(600.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "unwrap-long");
    let (camera, translate) = (label_at(&t, "Screenshot"), label_at(&t, "Translate"));
    assert!(
        camera.is_none() && translate.is_some(),
        "wrapped, the camera alone"
    );
    panel.show_settings(true);
    panel.edit_config(one("Fix"));
    panel.save_config();
    panel.show_settings(false);
    settle(&mut t);
    shot(&mut t, "unwrap-short");
    let camera = label_at(&t, "Screenshot").expect("room for Screenshot's label again");
    let translate = label_at(&t, "Translate").expect("Translate");
    assert!(
        translate.min_y() < camera.max_y() && translate.min_x() > camera.max_x(),
        "one row: {camera:?} {translate:?}"
    );
}

/// The header's Think writes `[llm] chat_reasoning` to config.toml and lights
/// up; a second press turns it off again.
pub fn think_in_the_header_writes_chat_reasoning(proxy: &LaunchProxy) {
    let panel = panel(proxy, "think", &[]);
    let file = blitznir_core::config::Paths::new(data_dir("think")).config();
    std::fs::write(&file, blitznir_core::config::DEFAULT_TOML).expect("config.toml");
    let shown = panel.clone();
    let app = move || ui::app(shown.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    assert!(!panel.ui().reasons, "off by default");
    press_label(&mut t, "Think");
    let written = |on: bool| {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        text.contains(&format!("chat_reasoning = {on}"))
    };
    wait_for("chat_reasoning = true", || written(true));
    assert!(panel.ui().reasons, "lit");
    settle(&mut t);
    shot(&mut t, "think-on");
    press_label(&mut t, "Think");
    wait_for("chat_reasoning = false", || written(false));
    assert!(!panel.ui().reasons);
    settle(&mut t);
    shot(&mut t, "think-off");
}
