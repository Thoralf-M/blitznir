//! One after another: what a finished job or request leaves behind must
//! not touch what came after it.

use super::kit::*;
use blitznir_app::panel::Mode;
use blitznir_app::panel::view::Busy;
use blitznir_core::lock;
use blitznir_core::platform::Platform;
use blitznir_core::platform::fake::FakePlatform;
use blitznir_core::tasks::Kind;
use freya::prelude::LaunchProxy;
use std::sync::Arc;
use std::time::Duration;

/// A job that starts while the last one still pastes (a dictation's rewrite,
/// say) keeps its busy: the finished one's end clears only its own.
pub fn a_job_started_mid_paste_stays_busy(proxy: &LaunchProxy) {
    let platform = Arc::new(FakePlatform::new());
    let cfg = local_config(&silent_endpoint());
    let panel = panel_over(proxy, "busy-handover", &[], platform.clone(), cfg);
    let next = Arc::downgrade(&panel);
    platform.on_type(move |_| {
        let Some(p) = next.upgrade() else {
            return Ok(());
        };
        if p.ui().busy == Some(Busy::Write) {
            p.rewrite_dictation("hello there".into(), true); // runs until Esc
            wait_for("the rewrite to start", || {
                p.ui().busy == Some(Busy::Rewrite)
            });
        }
        Ok(())
    });
    panel.write_out(Some("Out.".into()));
    wait_for("the paste", || platform.screen() == "Out.");
    std::thread::sleep(Duration::from_millis(300)); // the write-out's end
    assert_eq!(
        panel.ui().busy,
        Some(Busy::Rewrite),
        "the rewrite still runs"
    );
    panel.escape();
    assert!(panel.ui().busy.is_none(), "cancelled");
    assert_eq!(lock(&platform.pasted_into).len(), 1);
}

/// The model list asked for a server since dropped from config.toml comes
/// back last: it never replaces the list of the servers there now.
pub fn a_slow_model_list_never_replaces_a_newer_one(proxy: &LaunchProxy) {
    let server =
        |name: &str, url: &str| format!("[[endpoints]]\nname = \"{name}\"\nurl = \"{url}\"\n");
    let slow = failing_endpoint(Duration::from_millis(800));
    let platform = Arc::new(FakePlatform::new());
    let panel = panel_over(proxy, "models-race", &[], platform, local_config(&slow));
    panel.show_settings(true);
    panel.edit_config(server("slow", &slow));
    panel.save_config(); // asks the slow one
    panel.edit_config(server("gone", "http://127.0.0.1:9/v1"));
    panel.save_config(); // refused at once: the list says so
    let listed = |name: &str| panel.ui().models.iter().any(|r| r.endpoint == name);
    wait_for("the second list", || listed("gone"));
    std::thread::sleep(Duration::from_millis(1500)); // the slow one is back
    assert!(listed("gone") && !listed("slow"), "the newer list stays");
}

/// The hotkey again while a rewrite runs cancels it, and the next open
/// empties the composer: the typed instruction stays in the history, in a
/// row that says Cancelled where the answer would be.
pub fn a_rewrite_cut_short_by_a_new_open_keeps_its_prompt(proxy: &LaunchProxy) {
    const ASKED: &str = "turn this into a long poem about the sea";
    let platform = Arc::new(FakePlatform::new());
    let cfg = local_config(&silent_endpoint());
    let panel = panel_over(proxy, "cancel-kept", &[], platform, cfg);
    panel.open(Mode::Rewrite, Some("the waves".into()));
    wait_for("the selection", || panel.ui().selection.is_some());
    panel.set_composer(ASKED.into());
    panel.run(Kind::Rewrite, 0); // runs until cancelled
    wait_for("the rewrite", || panel.ui().busy == Some(Busy::Rewrite));
    panel.open(Mode::Rewrite, Some("something else".into()));
    wait_for("the new open", || {
        panel.ui().selection.as_deref() == Some("something else")
    });
    let ui = panel.ui();
    assert!(ui.busy.is_none() && ui.composer.text.is_empty());
    let row = ui
        .recent
        .iter()
        .find(|h| h.cancelled)
        .expect("a cancelled row");
    assert_eq!((row.text.as_str(), row.mode.as_str()), ("", "rewrite"));
    assert_eq!(row.original.as_deref(), Some("the waves"));
    assert!(row.prompt.as_deref().is_some_and(|p| p.contains(ASKED)));
}

/// A rewrite whose request fails and a chat turn cut short leave their
/// prompts in the history too, the failure with its error.
pub fn a_failed_rewrite_and_a_cancelled_chat_keep_their_prompts(proxy: &LaunchProxy) {
    const ASKED: &str = "make it formal";
    const SAID: &str = "what does this paragraph mean?";
    let platform = Arc::new(FakePlatform::new());
    let cfg = local_config(&failing_endpoint(Duration::from_millis(100)));
    let panel = panel_over(proxy, "fail-kept", &[], platform.clone(), cfg);
    panel.open(Mode::Rewrite, Some("hey you".into()));
    wait_for("the selection", || panel.ui().selection.is_some());
    panel.set_composer(ASKED.into());
    panel.run(Kind::Rewrite, 0);
    let failed = || panel.ui().recent.iter().any(|h| h.failed.is_some());
    wait_for("the failed row", failed);
    let ui = panel.ui();
    let row = &ui.recent[0];
    assert!(row.unanswered().is_some_and(|w| w.starts_with("Failed: ")));
    assert!(row.prompt.as_deref().is_some_and(|p| p.contains(ASKED)));

    let cfg = local_config(&silent_endpoint());
    let panel = panel_over(proxy, "chat-kept", &[], platform, cfg);
    panel.open(Mode::Chat, Some("a paragraph".into()));
    wait_for("the selection", || panel.ui().selection.is_some());
    panel.set_composer(SAID.into());
    panel.send(); // runs until cancelled
    wait_for("the chat turn", || panel.ui().busy == Some(Busy::Chat));
    panel.open(Mode::Chat, Some("other".into()));
    wait_for("the new open", || {
        panel.ui().selection.as_deref() == Some("other")
    });
    let ui = panel.ui();
    let row = ui
        .recent
        .iter()
        .find(|h| h.mode == "chat")
        .expect("a chat row");
    assert_eq!(row.prompt.as_deref(), Some(SAID));
    assert_eq!(row.original.as_deref(), Some("a paragraph"));
    assert_eq!(row.unanswered().as_deref(), Some("Cancelled"));
}

/// Settings' Save hands `[linux]` to the platform: the typing layout
/// changes without a restart.
pub fn a_saved_layout_reaches_the_platform_at_once(proxy: &LaunchProxy) {
    let platform = Arc::new(FakePlatform::new());
    let cfg = local_config(&silent_endpoint());
    let panel = panel_over(proxy, "layout-live", &[], platform.clone(), cfg);
    panel.show_settings(true);
    panel.edit_config("[linux]\nxkb_layout = \"de\"\n".into());
    panel.save_config();
    wait_for("the layout", || {
        lock(&platform.layout).as_deref() == Some("de")
    });
}

/// Where the window system gives no theme (Wayland), the desktop's dark
/// preference from the platform makes the panel dark.
pub fn the_desktops_dark_preference_makes_the_panel_dark(proxy: &LaunchProxy) {
    use blitznir_app::ui;
    use freya::prelude::{Platform, PreferredTheme, Size2D};
    use freya_testing::prelude::TestingRunner;
    let mut fake = FakePlatform::new();
    fake.dark = Some(true);
    let cfg = local_config(&silent_endpoint());
    let panel = panel_over(proxy, "dark-desktop", &[], Arc::new(fake), cfg);
    let app = move || ui::app(panel.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(760.0, 540.0), |_| {}, 1.0);
    settle(&mut t);
    let theme = t.run_in(|| *Platform::get().current_window().preferred_theme.read());
    assert!(matches!(theme, PreferredTheme::Dark));
}

/// Esc on a live dictation's rewrite leaves the dictation typed; the next
/// paste-back (here a Write out) must not take those characters back.
pub fn a_cancelled_dictation_rewrite_leaves_the_next_paste_alone(proxy: &LaunchProxy) {
    let platform = Arc::new(FakePlatform::new());
    let said = "hello there";
    platform.type_text(said).expect("typed");
    let cfg = local_config(&silent_endpoint());
    let panel = panel_over(proxy, "dictation-cancel", &[], platform.clone(), cfg);
    panel.rewrite_dictation(said.into(), true);
    wait_for("the rewrite to run", || panel.ui().busy.is_some());
    panel.escape();
    assert!(panel.ui().busy.is_none(), "cancelled");
    panel.write_out(Some(" Out.".into()));
    let pasted = || !lock(&platform.pasted_into).is_empty();
    wait_for("the paste", pasted);
    assert_eq!(platform.screen(), "hello there Out.");
}

/// Esc on a live dictation's rewrite while the effect's overlay has the key
/// focus: the app dictated into gets it back, with no panel shown.
pub fn a_cancelled_dictation_rewrite_gives_the_focus_back(proxy: &LaunchProxy) {
    use std::sync::atomic::Ordering;
    let platform = Arc::new(FakePlatform::new());
    let cfg = local_config(&silent_endpoint());
    let panel = panel_over(proxy, "dictation-refocus", &[], platform.clone(), cfg);
    let dictated_in = platform.focused_app();
    panel.rewrite_dictation("hello there".into(), true);
    wait_for("the rewrite to run", || panel.ui().busy.is_some());
    platform.ours_in_front.store(true, Ordering::SeqCst);
    panel.escape();
    wait_for("the focus back", || !lock(&platform.refocused).is_empty());
    assert_eq!(*lock(&platform.refocused), [dictated_in]);
    assert!(!panel.ui().shown, "no panel");
}

/// Esc on a batch dictation's rewrite: nothing was typed while it ran, so
/// what was said is pasted where it was dictated, as a failed rewrite does.
pub fn a_cancelled_batch_rewrite_puts_the_dictation_in(proxy: &LaunchProxy) {
    let platform = Arc::new(FakePlatform::new());
    let cfg = local_config(&silent_endpoint());
    let panel = panel_over(proxy, "batch-cancel", &[], platform.clone(), cfg);
    panel.rewrite_dictation("hello there".into(), false);
    wait_for("the rewrite to run", || panel.ui().busy.is_some());
    assert_eq!(platform.screen(), "", "batch holds it back");
    panel.escape();
    wait_for("the dictation pasted", || {
        platform.screen() == "hello there"
    });
    assert_eq!(lock(&platform.pasted_into).len(), 1, "once");
}
