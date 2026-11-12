//! `[panel] dictation_effect` in our own composer: a dictation that types
//! into the panel gets the wave under the composer's last line, its tip at
//! the text's end; it fades out when the recording ends.

use super::kit::*;
use blitznir_app::{
    panel::{Mode, Panel},
    ui,
};
use freya::prelude::{Area, LaunchProxy, Platform, PreferredTheme, Size2D, WritableUtils};
use freya_testing::prelude::*;
use std::sync::Arc;

const SAID: &str = "Dictated straight into the panel's own composer, \
                    word after word, until the line is full and wraps";
const MORE: &str = " and on";

/// The input's paragraph showing `text`: its last line's left, right,
/// bottom, in the window.
fn last_line(t: &TestingRunner, text: &str) -> (f32, f32, f32) {
    t.find(|node, element| {
        let p = Paragraph::try_downcast(element)?;
        // the input's own paragraph: the hidden copy carries a space more
        if p.spans.iter().map(|s| s.text.as_ref()).collect::<String>() != text {
            return None;
        }
        let laid = p.sk_paragraph.0.borrow();
        let laid = laid.as_ref()?;
        let s = laid.scale_factor as f32;
        let m = laid.paragraph.get_line_metrics().pop()?;
        let area = node.layout().area;
        let (pl, pt) = (laid.padding.left(), laid.padding.top());
        let left = area.min_x() + pl + m.left as f32 / s;
        let right = left + m.width as f32 / s;
        let bottom = area.min_y() + pt + (m.baseline + m.descent) as f32 / s;
        Some((left, right, bottom))
    })
    .expect("the composer's text laid out")
}

/// The band the wave runs in: under the last line, from its start to past the end.
fn band(t: &TestingRunner, text: &str) -> (Area, f32) {
    let (left, right, bottom) = last_line(t, text);
    let area = Area::new(
        (left - 4.0, bottom + 2.0).into(),
        (right - left + 40.0, 8.0).into(),
    );
    (area, right)
}

/// The rightmost column in `area` that differs from its top-left pixel.
fn lit_to(t: &mut TestingRunner, area: Area) -> Option<f32> {
    let png = t.render();
    let img = image::load_from_memory(png.as_bytes())
        .expect("a PNG")
        .to_rgba8();
    let bg = img.get_pixel(area.min_x() as u32, area.min_y() as u32).0;
    let differs = |x: u32| {
        (area.min_y() as u32..area.max_y() as u32).any(|y| {
            let px = img.get_pixel(x, y).0;
            (0..3).any(|i| px[i].abs_diff(bg[i]) > 6)
        })
    };
    (area.min_x() as u32..area.max_x() as u32)
        .rev()
        .find(|&x| differs(x))
        .map(|x| x as f32)
}

fn open(panel: &Arc<Panel>, t: &mut TestingRunner) {
    panel.open(Mode::Chat, Some(String::new()));
    wait_for("the open", || panel.ui().selection.is_some());
    settle(t);
}

/// Under the composer's last line while it records, tip at the end; moves
/// with new words; gone after the recording; dark and light.
pub fn the_dictation_wave_shows_under_the_composer(proxy: &LaunchProxy) {
    for dark in [true, false] {
        let name = if dark { "dark" } else { "light" };
        let panel = panel(proxy, &format!("dictation-{name}"), &[]);
        let p = panel.clone();
        let app = move || ui::app(p.clone());
        let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
        let theme = if dark {
            PreferredTheme::Dark
        } else {
            PreferredTheme::Light
        };
        t.run_in(|| Platform::get().current_window().preferred_theme.set(theme));
        open(&panel, &mut t);
        panel.set_dictating(true);
        animate(&mut t, 500);
        shot(&mut t, &format!("dictation-panel-{name}-empty"));
        t.write_text(SAID);
        settle(&mut t);
        assert_eq!(panel.ui().composer.text, SAID, "typed into the composer");
        animate(&mut t, 700);
        shot(&mut t, &format!("dictation-panel-{name}-wrapped"));
        let (area, end) = band(&t, SAID);
        let tip = lit_to(&mut t, area).expect("the wave under the last line");
        // the lit pixels' right end: the tip 7 pt past the text, its glow and
        // the specks ahead (up to ~8 pt) beyond it; a busy machine's frames
        // leave the spring a little further on
        assert!(
            (end - 2.0..end + 45.0).contains(&tip),
            "{name}: the tip near the text's end {end}: {tip}"
        );
        t.write_text(MORE);
        animate(&mut t, 700);
        let text = format!("{SAID}{MORE}");
        let (area, end2) = band(&t, &text);
        let tip2 = lit_to(&mut t, area).expect("the wave after more words");
        shot(&mut t, &format!("dictation-panel-{name}-more"));
        assert!(
            end2 > end && tip2 > tip,
            "{name}: the tip followed: {tip} -> {tip2}"
        );
        panel.set_dictating(false);
        animate(&mut t, 500);
        settle(&mut t);
        shot(&mut t, &format!("dictation-panel-{name}-done"));
        let (area, _) = band(&t, &text);
        let gone = lit_to(&mut t, area);
        assert!(gone.is_none(), "{name}: gone after the recording: {gone:?}");
    }
}

/// `[panel] dictation_effect = false`: nothing under the composer.
pub fn the_dictation_wave_can_be_turned_off(proxy: &LaunchProxy) {
    let mut cfg = local_config("http://127.0.0.1:9/v1");
    cfg.panel.dictation_effect = false;
    let platform = Arc::new(blitznir_core::platform::fake::FakePlatform::new());
    let panel = panel_over(proxy, "dictation-off", &[], platform, cfg);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    open(&panel, &mut t);
    panel.set_dictating(true);
    t.write_text(SAID);
    animate(&mut t, 500);
    let (area, _) = band(&t, SAID);
    assert_eq!(lit_to(&mut t, area), None, "no wave when it is off");
}

/// The composer's unseen copy (its height, the wave's caret) wraps where
/// the input does: typed a letter at a time past a line's end, the two
/// always have as many lines (the copy's room was 14 pt short).
pub fn the_composer_copy_wraps_as_the_input(proxy: &LaunchProxy) {
    let panel = panel(proxy, "copy-wraps", &[]);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    open(&panel, &mut t);
    let lines = |t: &TestingRunner, text: &str| {
        t.find(|_, element| {
            let p = Paragraph::try_downcast(element)?;
            let spans: String = p.spans.iter().map(|s| s.text.as_ref()).collect();
            let laid = p.sk_paragraph.0.borrow();
            (spans == text).then(|| laid.as_ref().map(|l| l.paragraph.line_number()))?
        })
        .unwrap_or_else(|| panic!("{text:?} laid out"))
    };
    let mut text = String::from("A line typed into the composer, a letter at a time: ");
    t.write_text(&text);
    for _ in 0..50 {
        t.write_text("x");
        text.push('x');
        settle(&mut t);
        let (shown, copy) = (lines(&t, &text), lines(&t, &format!("{text} ")));
        assert_eq!(shown, copy, "{} chars: the copy wraps alike", text.len());
    }
    assert!(lines(&t, &text) > 1, "past the line's end");
}
