//! An answer's thinking: the folded row above it, its box, and the trace
//! as it streams in.

use super::kit::*;
use blitznir_app::{panel::Mode, ui};
use blitznir_core::store::{Chat, Unthought};
use freya::prelude::{LaunchProxy, Size2D};
use freya_testing::prelude::*;
use std::time::Duration;

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

/// An answer's reasoning sits folded above it: one "Thinking" row, its
/// text hidden; a press shows the text, a second press hides it again.
pub fn an_answers_thinking_opens_and_folds_on_a_press(proxy: &LaunchProxy) {
    const TRACE: &str = "Wait, I'll check the sky first.";
    let mut chat = named("Why blue?");
    (chat.messages[1].thinking, chat.messages[1].unthought) = (TRACE.into(), Unthought::RanOut);
    let panel = panel(proxy, "thinking-fold", std::slice::from_ref(&chat));
    panel.open_chat(&chat.id);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "thinking-folded");
    let row = "Thinking (ran out, answered without it)";
    assert!(label_at(&t, row).is_some(), "the folded row");
    assert!(paragraph_with(&t, TRACE).is_none(), "its text hidden");
    press_label(&mut t, row);
    shot(&mut t, "thinking-open");
    assert!(paragraph_with(&t, TRACE).is_some(), "a press shows it");
    press_label(&mut t, row);
    shot(&mut t, "thinking-folded-again");
    assert!(
        paragraph_with(&t, TRACE).is_none(),
        "a second press hides it"
    );
}

/// Where the Thinking row is, after a press on it.
fn row_after_press(t: &mut TestingRunner) -> f32 {
    press_label(t, "Thinking");
    settle(t);
    label_at(t, "Thinking").expect("the row").min_y()
}

/// A long chat whose last, short answer has a trace: opening its row,
/// at the bottom or scrolled up, keeps the scroll where it is (the row
/// stays under the pointer), and so does folding it.
pub fn opening_a_thinking_row_keeps_the_scroll(proxy: &LaunchProxy) {
    let trace: String = (0..80)
        .map(|i| format!("Step {i}: check the sky.\n"))
        .collect();
    let mut chat = long_chat();
    chat.messages.push(message("user", "And why?".into()));
    let mut a = message("assistant", "Because of the air.".into());
    a.thinking = trace;
    chat.messages.push(a);
    let panel = panel(proxy, "thinking-scroll", std::slice::from_ref(&chat));
    panel.open_chat(&chat.id);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    let at = |t: &TestingRunner| label_at(t, "Thinking").expect("the row").min_y();
    let bottom = at(&t);
    assert!(bottom > WINDOW.1 * 0.5, "at the bottom: {bottom}");
    shot(&mut t, "thinking-scroll-bottom");
    let open = row_after_press(&mut t);
    shot(&mut t, "thinking-scroll-bottom-open");
    assert!(
        (open - bottom).abs() < 1.0,
        "opened at the bottom: {bottom} -> {open}"
    );
    let folded = row_after_press(&mut t);
    assert!(
        (folded - bottom).abs() < 1.0,
        "folded: {bottom} -> {folded}"
    );
    for _ in 0..5 {
        t.scroll((500.0, 250.0), (0.0, 20.0));
        settle(&mut t);
    }
    let mid = at(&t);
    assert!(mid > bottom + 50.0, "scrolled up: {mid}");
    let open = row_after_press(&mut t);
    shot(&mut t, "thinking-scroll-mid-open");
    assert!(
        (open - mid).abs() < 1.0,
        "opened mid-screen: {mid} -> {open}"
    );
    let folded = row_after_press(&mut t);
    assert!(
        (folded - mid).abs() < 1.0,
        "folded mid-screen: {mid} -> {folded}"
    );
}

/// The text of the paragraph that starts with `start`.
fn text_from(t: &TestingRunner, start: &str) -> Option<String> {
    t.find(|_, e| {
        let p = Paragraph::try_downcast(e)?;
        let text: String = p.spans.iter().map(|s| s.text.to_string()).collect();
        text.starts_with(start).then_some(text)
    })
}

/// An opened long trace sits in a box of its own scroll, under its row: a
/// wheel over it scrolls the trace, not the chat, and the Fold at its end
/// folds it.
pub fn a_long_trace_scrolls_in_its_box_and_folds_from_its_end(proxy: &LaunchProxy) {
    let mut chat = named("Why blue?");
    chat.messages[1].thinking = (0..80)
        .map(|i| format!("Step {i}: check the sky.\n"))
        .collect();
    let panel = panel(proxy, "thinking-box", std::slice::from_ref(&chat));
    panel.open_chat(&chat.id);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    press_label(&mut t, "Thinking");
    shot(&mut t, "thinking-box-open");
    let row = label_at(&t, "Thinking").expect("the row");
    let fold = label_at(&t, "Fold").expect("a Fold at the end");
    let tall = fold.min_y() - row.max_y();
    assert!(tall > 100.0 && tall < 260.0, "a bounded box: {tall}");
    let (_, text) = paragraph_with(&t, "Step 0:").expect("the trace");
    let inside = (f64::from(text.center().x), f64::from(row.max_y() + 60.0));
    for _ in 0..4 {
        t.scroll(inside, (0.0, -40.0));
        settle(&mut t);
    }
    shot(&mut t, "thinking-box-scrolled");
    let (_, moved) = paragraph_with(&t, "Step 0:").expect("the trace");
    assert!(
        moved.min_y() < text.min_y() - 50.0,
        "scrolled inside: {text:?} -> {moved:?}"
    );
    let still = label_at(&t, "Thinking").expect("the row");
    assert!(
        (still.min_y() - row.min_y()).abs() < 1.0,
        "the chat stayed: {row:?} -> {still:?}"
    );
    press_label(&mut t, "Fold");
    shot(&mut t, "thinking-box-folded");
    assert!(
        paragraph_with(&t, "Step 0:").is_none(),
        "folded from its end"
    );
    assert!(label_at(&t, "Fold").is_none());
}

/// The row shows up, folded, as soon as reasoning streams in; opened, its
/// trace grows as it comes, its box following its newest line; it stays,
/// still open, once the answer is in.
pub fn the_thinking_row_streams_live(proxy: &LaunchProxy) {
    // a line a word (JSON's \n), more lines than the box holds
    let words: Vec<String> = (1..=40).map(|i| format!("w{i:02}\\n")).collect();
    let trace: &'static str = Box::leak(words.join(" ").into_boxed_str());
    let url = thinking_endpoint(trace, "Blue.", Duration::from_millis(80));
    let panel = panel_over(proxy, "thinking-live", &[], fake(), local_config(&url));
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    panel.open(Mode::Chat, Some(String::new()));
    wait_for("the open", || {
        panel.ui().chatting && panel.ui().selection.is_some()
    });
    panel.set_composer("Why is the sky blue?".into());
    panel.send();
    let poll = |t: &mut TestingRunner, what, done: &dyn Fn(&TestingRunner) -> bool| {
        for _ in 0..300 {
            settle(t);
            if done(t) {
                return;
            }
        }
        panic!("waited for {what}");
    };
    poll(&mut t, "the row", &|t| label_at(t, "Thinking").is_some());
    assert!(
        panel.ui().streaming.is_some_and(|s| s.is_empty()),
        "before the answer"
    );
    assert!(text_from(&t, "w01").is_none(), "folded");
    shot(&mut t, "thinking-live-folded");
    press_label(&mut t, "Thinking");
    let first = text_from(&t, "w01").expect("opened").len();
    poll(&mut t, "more than the box holds", &|t| {
        text_from(t, "w01").is_some_and(|s| s.len() > first + 8 && s.contains("w30"))
    });
    // a line in after the last settle is a frame ahead of the box's jump
    // to it: the box catches up, not stays a line behind
    let gap = |t: &TestingRunner| {
        let (_, text) = paragraph_with(t, "w01").expect("the trace");
        label_at(t, "Fold").expect("its Fold").min_y() - text.max_y()
    };
    poll(&mut t, "the box at its newest line", &|t| {
        (-2.0..20.0).contains(&gap(t))
    });
    assert!(panel.ui().streaming.is_some(), "caught up while it streams");
    shot(&mut t, "thinking-live-open");
    wait_for("the answer", || panel.ui().streaming.is_none());
    settle(&mut t);
    settle(&mut t);
    shot(&mut t, "thinking-live-done");
    let all = (1..=40)
        .map(|i| format!("w{i:02}"))
        .collect::<Vec<_>>()
        .join("\n ");
    assert_eq!(text_from(&t, "w01"), Some(all), "still open, all of it");
    assert!(paragraph_with(&t, "Blue.").is_some());
}
