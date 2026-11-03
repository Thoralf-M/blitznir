//! `[panel] thinking_effect`: the wave over a chat answer while it is asked
//! and streams in, gone once the answer is in.

use super::kit::*;
use blitznir_app::panel::{Mode, Panel};
use blitznir_app::ui;
use freya::prelude::{Area, LaunchProxy, Platform, PreferredTheme, Size2D, WritableUtils};
use freya_testing::prelude::*;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

const ANSWER: &str = "One two three four five six seven eight nine ten";

/// The colours in `area` of the frame: one where nothing is drawn.
fn colours(t: &mut TestingRunner, area: Area) -> usize {
    let png = t.render();
    let img = image::load_from_memory(png.as_bytes())
        .expect("a PNG")
        .to_rgba8();
    let mut seen = HashSet::new();
    for y in area.min_y() as u32..area.max_y() as u32 {
        for x in area.min_x() as u32..area.max_x() as u32 {
            seen.insert(img.get_pixel(x, y).0);
        }
    }
    seen.len()
}

/// The answer's first line, from just right of its text to past the tip.
fn ahead(t: &TestingRunner, text: &str, from: f32) -> Area {
    let (_, at) = paragraph_with(t, text).expect("the answer laid out");
    Area::new(
        (at.min_x() + from, at.min_y()).into(),
        (110.0, at.height()).into(),
    )
}

/// The answer's slot before the first word: under the last "Assistant".
fn slot(t: &TestingRunner) -> Area {
    let all = t.find_many(|node, element| {
        Label::try_downcast(element)
            .filter(|l| l.text == "Assistant")
            .map(|_| node.layout().area)
    });
    let a = *all.last().expect("the streaming card's head");
    Area::new(
        (a.min_x() + 60.0, a.max_y() + 4.0).into(),
        (110.0, 16.0).into(),
    )
}

fn ask(panel: &Arc<Panel>) {
    panel.open(Mode::Chat, Some(String::new()));
    wait_for("the open", || {
        let ui = panel.ui();
        ui.chatting && ui.selection.is_some()
    });
    panel.set_composer("Count to ten".into());
    panel.send();
    wait_for("the ask", || panel.ui().streaming.is_some());
}

fn poll_for(t: &mut TestingRunner, what: &str, mut done: impl FnMut(&mut TestingRunner) -> bool) {
    for _ in 0..200 {
        settle(t);
        if done(t) {
            return;
        }
    }
    panic!("waited for {what}");
}

/// In the slot while the model thinks, gone once words stream, dark and light.
pub fn the_thinking_wave_shows_while_an_answer_streams(proxy: &LaunchProxy) {
    for dark in [true, false] {
        let name = if dark { "dark" } else { "light" };
        // long enough that a busy machine still sees the wave before the first word
        let first = Duration::from_millis(2000);
        let url = slow_endpoint(ANSWER, first, Duration::from_millis(600));
        let panel = panel_over(
            proxy,
            &format!("thinking-{name}"),
            &[],
            fake(),
            local_config(&url),
        );
        let p = panel.clone();
        let app = move || ui::app(p.clone());
        let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
        let theme = if dark {
            PreferredTheme::Dark
        } else {
            PreferredTheme::Light
        };
        t.run_in(|| Platform::get().current_window().preferred_theme.set(theme));
        settle(&mut t);
        ask(&panel);
        poll_for(&mut t, "the wave in the slot", |t| {
            let area = slot(t);
            colours(t, area) > 3
        });
        animate(&mut t, 400);
        shot(&mut t, &format!("thinking-{name}-waiting"));
        poll_for(&mut t, "words", |_| {
            panel.ui().streaming.is_some_and(|s| s.len() > 18)
        });
        // the first words: it fades out where it waited, never chasing the text
        animate(&mut t, 500);
        let area = ahead(&t, "One two", 150.0);
        let lit = colours(&mut t, area);
        shot(&mut t, &format!("thinking-{name}-streaming"));
        // the words grow into the area: their antialiasing is a few dozen
        // colours; a wave chasing them was over 500
        assert!(lit < 100, "{name}: gone once words stream: {lit} colours");
        poll_for(&mut t, "the answer", |_| panel.ui().streaming.is_none());
        t.poll(Duration::from_millis(30), Duration::from_millis(300));
        let area = ahead(&t, "ten", 330.0);
        let after = colours(&mut t, area);
        shot(&mut t, &format!("thinking-{name}-done"));
        assert_eq!(after, 1, "{name}: nothing once the answer is in");
    }
}
