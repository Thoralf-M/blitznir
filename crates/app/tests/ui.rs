//! The panel's UI headless (freya-testing): its real components over core's
//! Fake platform and made-up chats in a temporary data dir. No window,
//! nothing on the screen, no input to or from anything else. Its own main:
//! the panel needs a LaunchProxy, and macOS makes the event loop behind one
//! only on the main thread (the loop never runs).
// a test runner's report is its output: the app's no-println rule is for the app
#![allow(clippy::print_stdout)]

#[path = "ui/buttons.rs"]
mod buttons;
#[path = "ui/chat.rs"]
mod chat;
#[path = "ui/dictation.rs"]
mod dictation;
#[path = "ui/find.rs"]
mod find;
#[path = "ui/jobs.rs"]
mod jobs;
#[path = "ui/kit.rs"]
mod kit;
#[path = "ui/pages.rs"]
mod pages;
#[path = "ui/quick.rs"]
mod quick;
#[path = "ui/record.rs"]
mod record;
#[path = "ui/select.rs"]
mod select;
#[path = "ui/suggest.rs"]
mod suggest;
#[path = "ui/thinking.rs"]
mod thinking;

use blitznir_app::ui;
use blitznir_core::platform::fake::FakePlatform;
use blitznir_core::store::State;
use freya::elements::paragraph::ParagraphElement;
use freya::prelude::{LaunchProxy, NativeEvent, Size2D};
use freya::winit::event_loop::EventLoop;
use freya_testing::prelude::*;
use kit::*;
use std::sync::Arc;

/// Over the transcript, right of the sidebar.
const TRANSCRIPT: (f64, f64) = (500.0, 250.0);

/// A test: its name, and its body over the shared proxy.
type Test = (&'static str, fn(&LaunchProxy));

/// Every test, run in this order on the one proxy.
const TESTS: [Test; 59] = [
    (
        "the_dictation_wave_shows_under_the_composer",
        dictation::the_dictation_wave_shows_under_the_composer,
    ),
    (
        "the_dictation_wave_can_be_turned_off",
        dictation::the_dictation_wave_can_be_turned_off,
    ),
    (
        "the_thinking_wave_shows_while_an_answer_streams",
        thinking::the_thinking_wave_shows_while_an_answer_streams,
    ),
    (
        "a_quick_translation_replaces_the_selection",
        quick::a_quick_translation_replaces_the_selection,
    ),
    (
        "a_quick_translation_on_a_page_shows_the_panel",
        quick::a_quick_translation_on_a_page_shows_the_panel,
    ),
    (
        "a_buttons_hotkey_runs_that_button",
        buttons::a_buttons_hotkey_runs_that_button,
    ),
    (
        "the_button_editor_has_the_hotkey",
        buttons::the_button_editor_has_the_hotkey,
    ),
    (
        "a_panel_shown_before_any_open_has_its_sidebar",
        a_panel_shown_before_any_open_has_its_sidebar,
    ),
    (
        "a_cancelled_dictation_rewrite_leaves_the_next_paste_alone",
        jobs::a_cancelled_dictation_rewrite_leaves_the_next_paste_alone,
    ),
    (
        "a_slow_scroll_up_near_the_bottom_stays_where_it_went",
        a_slow_scroll_up_near_the_bottom_stays_where_it_went,
    ),
    (
        "a_settings_help_text_can_be_selected",
        a_settings_help_text_can_be_selected,
    ),
    (
        "a_cancelled_batch_rewrite_puts_the_dictation_in",
        jobs::a_cancelled_batch_rewrite_puts_the_dictation_in,
    ),
    (
        "a_suggested_rewrite_leaves_the_dictation_until_taken",
        suggest::a_suggested_rewrite_leaves_the_dictation_until_taken,
    ),
    (
        "a_suggested_batch_rewrite_pastes_the_dictation",
        suggest::a_suggested_batch_rewrite_pastes_the_dictation,
    ),
    (
        "the_box_switch_writes_the_setting",
        suggest::the_box_switch_writes_the_setting,
    ),
    (
        "the_box_shows_either_text_and_the_switch",
        suggest::the_box_shows_either_text_and_the_switch,
    ),
    (
        "record_takes_the_pressed_keys_name",
        record::record_takes_the_pressed_keys_name,
    ),
    ("the_form_saves_itself", record::the_form_saves_itself),
    (
        "a_job_delays_the_autosave",
        record::a_job_delays_the_autosave,
    ),
    (
        "settings_scroll_from_the_side_and_the_margins",
        pages::settings_scroll_from_the_side_and_the_margins,
    ),
    (
        "settings_search_narrows_the_form",
        record::settings_search_narrows_the_form,
    ),
    (
        "the_sidebar_keeps_the_chats_and_folds_lists",
        record::the_sidebar_keeps_the_chats_and_folds_lists,
    ),
    (
        "the_chats_heading_stays_in_a_short_full_window",
        record::the_chats_heading_stays_in_a_short_full_window,
    ),
    (
        "settings_about_names_the_build_and_credits",
        record::settings_about_names_the_build_and_credits,
    ),
    (
        "a_tall_image_gets_a_narrow_thumbnail",
        suggest::a_tall_image_gets_a_narrow_thumbnail,
    ),
    (
        "translate_with_nothing_selected_translates_the_field",
        suggest::translate_with_nothing_selected_translates_the_field,
    ),
    (
        "a_quick_translation_on_a_page_shows_the_box",
        quick::a_quick_translation_on_a_page_shows_the_box,
    ),
    (
        "a_scroll_mid_drag_extends_the_selection",
        record::a_scroll_mid_drag_extends_the_selection,
    ),
    (
        "a_drag_from_beside_the_text_selects",
        select::a_drag_from_beside_the_text_selects,
    ),
    (
        "the_pointer_is_a_beam_only_over_text",
        select::the_pointer_is_a_beam_only_over_text,
    ),
    (
        "a_drag_up_through_about_selects_its_headings",
        select::a_drag_up_through_about_selects_its_headings,
    ),
    (
        "the_config_tab_keeps_the_search_and_about",
        find::the_config_tab_keeps_the_search_and_about,
    ),
    (
        "the_composer_tools_never_overlap",
        buttons::the_composer_tools_never_overlap,
    ),
    (
        "settings_are_pages_with_advanced_folded",
        pages::settings_are_pages_with_advanced_folded,
    ),
    (
        "command_f_in_the_editor_searches",
        pages::command_f_in_the_editor_searches,
    ),
    (
        "the_model_tooltip_leaves_the_open_list_alone",
        buttons::the_model_tooltip_leaves_the_open_list_alone,
    ),
    (
        "the_chat_counts_clear_the_scrollbar",
        pages::the_chat_counts_clear_the_scrollbar,
    ),
    (
        "a_prompt_edit_goes_to_the_open_button",
        buttons::a_prompt_edit_goes_to_the_open_button,
    ),
    (
        "command_f_shows_the_sidebar_and_searches",
        pages::command_f_shows_the_sidebar_and_searches,
    ),
    (
        "a_search_holds_the_history_open",
        pages::a_search_holds_the_history_open,
    ),
    (
        "tab_goes_from_field_to_field",
        select::tab_goes_from_field_to_field,
    ),
    (
        "an_error_above_the_composer_selects",
        select::an_error_above_the_composer_selects,
    ),
    (
        "think_in_the_header_writes_chat_reasoning",
        buttons::think_in_the_header_writes_chat_reasoning,
    ),
    (
        "a_chat_answer_keeps_its_code_fences",
        chat::a_chat_answer_keeps_its_code_fences,
    ),
    (
        "a_rename_stays_with_its_chat",
        chat::a_rename_stays_with_its_chat,
    ),
    (
        "the_buttons_table_fits_a_narrow_window",
        buttons::the_buttons_table_fits_a_narrow_window,
    ),
    (
        "a_failed_batch_rewrite_puts_the_dictation_in",
        suggest::a_failed_batch_rewrite_puts_the_dictation_in,
    ),
    (
        "the_composer_copy_wraps_as_the_input",
        dictation::the_composer_copy_wraps_as_the_input,
    ),
    (
        "a_long_title_leaves_the_head_its_delete",
        chat::a_long_title_leaves_the_head_its_delete,
    ),
    (
        "a_job_started_mid_paste_stays_busy",
        jobs::a_job_started_mid_paste_stays_busy,
    ),
    (
        "a_slow_model_list_never_replaces_a_newer_one",
        jobs::a_slow_model_list_never_replaces_a_newer_one,
    ),
    (
        "a_rewrite_cut_short_by_a_new_open_keeps_its_prompt",
        jobs::a_rewrite_cut_short_by_a_new_open_keeps_its_prompt,
    ),
    (
        "a_failed_rewrite_and_a_cancelled_chat_keep_their_prompts",
        jobs::a_failed_rewrite_and_a_cancelled_chat_keep_their_prompts,
    ),
    (
        "a_saved_layout_reaches_the_platform_at_once",
        jobs::a_saved_layout_reaches_the_platform_at_once,
    ),
    (
        "the_desktops_dark_preference_makes_the_panel_dark",
        jobs::the_desktops_dark_preference_makes_the_panel_dark,
    ),
    (
        "a_removal_above_keeps_the_open_prompt",
        buttons::a_removal_above_keeps_the_open_prompt,
    ),
    (
        "a_selection_goes_with_its_chat",
        chat::a_selection_goes_with_its_chat,
    ),
    (
        "the_buttons_row_unwraps_once_it_fits",
        buttons::the_buttons_row_unwraps_once_it_fits,
    ),
    (
        "a_record_ends_with_settings",
        pages::a_record_ends_with_settings,
    ),
];

fn main() {
    let Ok(event_loop) = EventLoop::<NativeEvent>::with_user_event().build() else {
        println!("ui tests skipped: no event loop here (no display)");
        return;
    };
    let proxy = LaunchProxy(event_loop.create_proxy());
    for (name, test) in TESTS {
        test(&proxy);
        println!("test {name} ... ok");
    }
    println!("ui: {} passed", TESTS.len());
}

/// Where the answer's last line is, in the window.
fn last_line_y(t: &TestingRunner) -> f32 {
    let has_it = |p: &ParagraphElement| p.spans.iter().any(|s| s.text.contains(LAST_LINE));
    t.find(|node, element| {
        Paragraph::try_downcast(element)
            .filter(has_it)
            .map(|_| node.layout().area.min_y())
    })
    .expect("the answer's last line is laid out")
}

/// The transcript keeps to its bottom while an answer streams in; that hold
/// must not pull back a reader who scrolls up from it in small steps.
fn a_slow_scroll_up_near_the_bottom_stays_where_it_went(proxy: &LaunchProxy) {
    let chat = long_chat();
    let panel = panel(proxy, "scroll", std::slice::from_ref(&chat));
    panel.open_chat(&chat.id);
    let app = move || ui::app(panel.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "scroll-bottom");
    let bottom = last_line_y(&t);
    assert!(bottom < WINDOW.1, "a chat opens at its bottom: {bottom}");
    for _ in 0..10 {
        t.scroll(TRANSCRIPT, (0.0, 8.0));
        settle(&mut t);
    }
    shot(&mut t, "scroll-up");
    let moved = last_line_y(&t) - bottom;
    assert!(
        moved >= 70.0,
        "10 steps of 8 px up moved the text {moved} px"
    );
}

/// A field's help in the Settings form is text to copy, not a picture of it:
/// a double click selects a word of it.
fn a_settings_help_text_can_be_selected(proxy: &LaunchProxy) {
    const HELP: &str = "Up to 6, shown under the text field";
    let panel = panel(proxy, "help", &[]);
    panel.show_settings(true);
    let app = move || ui::app(panel.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "settings-form");
    let (lit, at) = paragraph_with(&t, HELP).expect("the help is selectable text");
    assert!(lit.is_empty(), "nothing selected yet");
    let word = (f64::from(at.min_x()) + 20.0, f64::from(at.center().y));
    t.move_cursor(word); // hovered first, as a real pointer is
    t.click_cursor(word);
    t.click_cursor(word);
    settle(&mut t);
    let (lit, _) = paragraph_with(&t, HELP).expect("still there");
    assert!(
        !lit.is_empty(),
        "a double click selected a word of the help"
    );
}

/// The quick action shows the panel when its paste fails, without an open
/// (which lays it out from state.json): the sidebar, model and buttons are
/// there all the same.
fn a_panel_shown_before_any_open_has_its_sidebar(proxy: &LaunchProxy) {
    let cfg = local_config("http://127.0.0.1:9/v1");
    let platform = Arc::new(FakePlatform::new());
    let panel = panel_over(proxy, "first-show", &[long_chat()], platform, cfg);
    let ui = panel.ui();
    assert_eq!(ui.side_width, State::SIDE_DEFAULT);
    assert_eq!(ui.picked, ("mock".to_string(), "m".to_string()));
    assert!(!ui.actions.is_empty(), "the buttons");
    let app = move || ui::app(panel.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "first-show");
    let (_, at) = paragraph_with(&t, "Which setter pattern?").expect("the chat in the sidebar");
    assert!(
        at.width() > 100.0,
        "a sidebar row as wide as the sidebar: {at:?}"
    );
}
