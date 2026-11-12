//! The panel's UI headless (freya-testing): its real components over core's Fake platform and
//! made-up chats in a temporary data dir. No window, nothing on the screen, no input to or from
//! anything else. Its own main: the panel needs a LaunchProxy, and macOS makes the event loop
//! behind one only on the main thread (the loop never runs).
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
#[path = "ui/reasoning.rs"]
mod reasoning;
#[path = "ui/record.rs"]
mod record;
#[path = "ui/retry.rs"]
mod retry;
#[path = "ui/select.rs"]
mod select;
#[path = "ui/side.rs"]
mod side;
#[path = "ui/suggest.rs"]
mod suggest;
#[path = "ui/thinking.rs"]
mod thinking;
#[path = "ui/view.rs"]
mod view;

use blitznir_app::ui;
use blitznir_core::platform::fake::FakePlatform;
use blitznir_core::store::State;
use freya::prelude::{LaunchProxy, NativeEvent, Size2D};
use freya::winit::event_loop::EventLoop;
use freya_testing::prelude::*;
use kit::*;
use std::sync::Arc;

/// A test: its name, and its body over the shared proxy.
type Test = (&'static str, fn(&LaunchProxy));

/// Every test, run in this order on the one proxy.
const TESTS: [Test; 77] = [
    (
        "a_translation_has_no_retry",
        retry::a_translation_has_no_retry,
    ),
    (
        "retry_replaces_the_last_answer",
        retry::retry_replaces_the_last_answer,
    ),
    (
        "a_chat_image_copies_and_shows_as_attached",
        retry::a_chat_image_copies_and_shows_as_attached,
    ),
    (
        "the_edge_over_history_drags_its_share",
        side::the_edge_over_history_drags_its_share,
    ),
    (
        "a_double_click_selects_a_word_a_triple_its_paragraph",
        view::a_double_click_selects_a_word_a_triple_its_paragraph,
    ),
    (
        "a_drag_selects_a_replacements_before_and_after",
        view::a_drag_selects_a_replacements_before_and_after,
    ),
    (
        "a_drag_in_shown_text_past_the_bottom_scrolls_and_selects_on",
        view::a_drag_in_shown_text_past_the_bottom_scrolls_and_selects_on,
    ),
    (
        "a_drag_past_the_bottom_scrolls_and_selects_on",
        view::a_drag_past_the_bottom_scrolls_and_selects_on,
    ),
    (
        "a_pressed_image_shows_large",
        view::a_pressed_image_shows_large,
    ),
    (
        "send_again_wraps_under_its_note",
        view::send_again_wraps_under_its_note,
    ),
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
        "a_mouse_selection_offers_translate",
        quick::a_mouse_selection_offers_translate,
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
        "a_cancelled_dictation_rewrite_gives_the_focus_back",
        jobs::a_cancelled_dictation_rewrite_gives_the_focus_back,
    ),
    (
        "a_slow_scroll_up_near_the_bottom_stays_where_it_went",
        view::a_slow_scroll_up_near_the_bottom_stays_where_it_went,
    ),
    (
        "the_settings_switches_are_slim",
        pages::the_settings_switches_are_slim,
    ),
    (
        "a_settings_help_text_can_be_selected",
        pages::a_settings_help_text_can_be_selected,
    ),
    (
        "a_cancelled_batch_rewrite_puts_the_dictation_in",
        jobs::a_cancelled_batch_rewrite_puts_the_dictation_in,
    ),
    (
        "a_dictation_offers_its_rewrite_on_click",
        suggest::a_dictation_offers_its_rewrite_on_click,
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
        "a_failed_rewrite_keeps_its_prompt_a_cancelled_chat_turn_its_chat",
        jobs::a_failed_rewrite_keeps_its_prompt_a_cancelled_chat_turn_its_chat,
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
        "an_answers_thinking_opens_and_folds_on_a_press",
        reasoning::an_answers_thinking_opens_and_folds_on_a_press,
    ),
    (
        "opening_a_thinking_row_keeps_the_scroll",
        reasoning::opening_a_thinking_row_keeps_the_scroll,
    ),
    (
        "a_long_trace_scrolls_in_its_box_and_folds_from_its_end",
        reasoning::a_long_trace_scrolls_in_its_box_and_folds_from_its_end,
    ),
    (
        "the_thinking_row_streams_live",
        reasoning::the_thinking_row_streams_live,
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
