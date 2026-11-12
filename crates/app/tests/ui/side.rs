//! The sidebar's edge between the chats and History: a drag moves it, and
//! state.json keeps where.

use super::kit::*;
use blitznir_app::{panel::List, ui};
use blitznir_core::store::{Chat, HistoryKind, Store};
use freya::prelude::{LaunchProxy, Size2D};
use freya_testing::prelude::*;

/// Room for History to grow by the drag, short of its ten rows (the test
/// store keeps ten): neither limit is what is tested.
const TALL: f32 = 700.0;
const UP: f32 = 60.0;

/// The History heading's top, in the window.
fn history_y(t: &TestingRunner) -> f32 {
    label_where(t, |s| s.starts_with("History ·"))
        .expect("the History heading")
        .min_y()
}

/// The edge's middle: over the heading's padding (4), its button's (1)
/// and half the edge (4.5).
fn edge_at(t: &TestingRunner) -> (f64, f64) {
    (100.0, f64::from(history_y(t)) - 10.0)
}

/// Many chats and dictations, History open: a drag of the edge over it
/// [`UP`] pt up gives History that much more, and state.json keeps it; a
/// drag far down leaves History its headings.
pub fn the_edge_over_history_drags_its_share(proxy: &LaunchProxy) {
    let chats: Vec<Chat> = (0..12)
        .map(|i| {
            let mut c = Chat::new();
            c.name = format!("Chat number {i}");
            c.messages = vec![message("user", "hi".into())];
            c
        })
        .collect();
    let seed = |s: &Store| {
        for i in 0..20 {
            s.add_history(&format!("dictated line {i}"), "live")
                .expect("a dictation");
        }
    };
    let cfg = blitznir_core::config::Config::default();
    let panel = panel_seeded(proxy, "history-edge", &chats, fake(), cfg, seed);
    panel.open_history(true);
    panel.fold_list(List::Rows(HistoryKind::Replacement)); // one list to grow
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, TALL), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "history-edge-before");
    let y0 = history_y(&t);
    let edge = edge_at(&t);
    let drag = |t: &mut TestingRunner, to: f64| {
        t.move_cursor(edge);
        t.press_cursor(edge);
        settle(t);
        t.move_cursor((edge.0, to));
        settle(t);
        t.release_cursor((edge.0, to));
        settle(t);
    };
    drag(&mut t, edge.1 - f64::from(UP));
    shot(&mut t, "history-edge-up");
    let up = y0 - history_y(&t);
    assert!(
        (UP - 4.0..=UP + 4.0).contains(&up),
        "History grew by the drag: {up}"
    );
    let share = panel.ui().history_share.expect("kept in state.json");
    assert!(share > 0.0, "{share}");
    let edge = edge_at(&t);
    t.move_cursor(edge);
    t.press_cursor(edge);
    settle(&mut t);
    t.move_cursor((edge.0, f64::from(TALL) + 200.0));
    settle(&mut t);
    t.release_cursor((edge.0, f64::from(TALL)));
    settle(&mut t);
    shot(&mut t, "history-edge-down");
    let heading = |start: &str| label_where(&t, |s| s.starts_with(start)).expect(start);
    let rows = heading("Replacements ·").min_y() - heading("Dictations ·").max_y();
    assert!(
        rows < 20.0,
        "History down to its headings: {rows} pt of rows"
    );
    assert!(history_y(&t) > y0, "the chats got the room");
}
