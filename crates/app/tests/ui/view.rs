//! The transcript as it is looked at: its scroll, a selecting drag past its
//! edge, an image shown large, a failed turn's Send again in a narrow window.

use super::kit::*;
use blitznir_app::panel::Mode;
use blitznir_app::ui;
use blitznir_core::lock;
use blitznir_core::platform::fake::FakePlatform;
use blitznir_core::store::Unanswered;
use freya::elements::image::Image;
use freya::elements::paragraph::ParagraphElement;
use freya::prelude::{Area, Key, LaunchProxy, NamedKey, Size2D};
use freya_testing::prelude::*;
use std::sync::Arc;
use std::time::Duration;

/// Over the transcript, right of the sidebar.
const TRANSCRIPT: (f64, f64) = (500.0, 250.0);

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
pub fn a_slow_scroll_up_near_the_bottom_stays_where_it_went(proxy: &LaunchProxy) {
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

/// The pictures laid out (not the icons), largest first.
fn images(t: &TestingRunner) -> Vec<Area> {
    let mut all = t.find_many(|node, e| {
        let area = node.layout().area;
        Image::try_downcast(e)
            .filter(|_| area.height() > 40.0)
            .map(|_| area)
    });
    all.sort_by(|a, b| b.width().total_cmp(&a.width()));
    all
}

/// A drag from the chat's top held below the transcript scrolls it, and
/// the selection grows with it down to the answer's last line.
pub fn a_drag_past_the_bottom_scrolls_and_selects_on(proxy: &LaunchProxy) {
    let chat = long_chat();
    let panel = panel(proxy, "drag-scroll", std::slice::from_ref(&chat));
    panel.open_chat(&chat.id);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    t.scroll(TRANSCRIPT, (0.0, 20000.0)); // to the top
    settle(&mut t);
    let (_, first) = paragraph_with(&t, "better pattern").expect("the question at the top");
    let start = (f64::from(first.min_x()) + 2.0, f64::from(first.center().y));
    t.move_cursor(start);
    t.press_cursor(start);
    settle(&mut t);
    let below = (start.0, f64::from(WINDOW.1) - 10.0); // over the composer
    t.move_cursor(below);
    t.poll(Duration::from_millis(16), Duration::from_millis(3000));
    shot(&mut t, "drag-scroll");
    let (lit, at) = paragraph_with(&t, LAST_LINE).expect("still laid out");
    t.release_cursor(below);
    settle(&mut t);
    assert!(at.max_y() < WINDOW.1 - 60.0, "scrolled to the end: {at:?}");
    assert!(!lit.is_empty(), "the selection reached the last line");
}

/// A press on an image shows it large over the window, at most its own size;
/// Esc closes it, as a press on it does.
pub fn a_pressed_image_shows_large(proxy: &LaunchProxy) {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(400, 300, image::Rgb([40, 120, 200]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("a PNG");
    let platform = Arc::new(FakePlatform::new());
    *lock(&platform.image) = Some(png);
    let cfg = local_config(&silent_endpoint());
    let panel = panel_over(proxy, "image-large", &[], platform, cfg);
    panel.open(Mode::Chat, Some(String::new()));
    wait_for("the open", || panel.ui().shown);
    panel.paste_image(true);
    wait_for("the image attached", || panel.ui().images.len() == 1);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    let thumb = images(&t)[0];
    t.click_cursor((f64::from(thumb.min_x()) + 10.0, f64::from(thumb.center().y)));
    settle(&mut t);
    shot(&mut t, "image-large");
    let large = images(&t)[0];
    assert_eq!(
        (large.width(), large.height()),
        (400.0, 300.0),
        "its own size"
    );
    t.press_key(Key::Named(NamedKey::Escape));
    settle(&mut t);
    assert_eq!(images(&t).len(), 1, "closed: the thumbnail alone");
    assert!(panel.ui().shown, "Esc closed the image, not the panel");
    t.click_cursor((f64::from(thumb.min_x()) + 10.0, f64::from(thumb.center().y)));
    settle(&mut t);
    let large = images(&t)[0];
    t.click_cursor((f64::from(large.center().x), f64::from(large.center().y)));
    settle(&mut t);
    assert_eq!(images(&t).len(), 1, "a press closed it");
    t.click_cursor((f64::from(thumb.min_x()) + 10.0, f64::from(thumb.center().y)));
    settle(&mut t);
    panel.escape(); // the panel hides: it comes back without the image
    wait_for("hidden", || !panel.ui().shown);
    settle(&mut t);
    assert!(
        images(&t).iter().all(|a| a.height() <= 90.0),
        "closed with the panel"
    );
    // a Retina screen: its 400 px are 200 points, laid out in device pixels
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 2.0);
    settle(&mut t);
    let thumb = images(&t)[0];
    t.click_cursor((f64::from(thumb.min_x()) + 10.0, f64::from(thumb.center().y)));
    settle(&mut t);
    let large = images(&t)[0];
    assert_eq!(
        (large.width(), large.height()),
        (400.0, 300.0),
        "pixel for pixel, not 2x"
    );
}

/// A chat turn the server never answered, in a narrow window: Send again
/// goes whole under the note, not a letter a line beside it.
pub fn send_again_wraps_under_its_note(proxy: &LaunchProxy) {
    let cfg = local_config("http://127.0.0.1:9/v1");
    let panel = panel_over(proxy, "send-again-narrow", &[], fake(), cfg);
    panel.open(Mode::Chat, Some(String::new()));
    wait_for("the open", || panel.ui().chatting);
    panel.set_composer("Are you there?".into());
    panel.send();
    wait_for("the failure", || {
        let ui = panel.ui();
        ui.busy.is_none() && matches!(ui.messages[..], [ref m] if m.unanswered.is_some())
    });
    assert!(matches!(
        panel.ui().messages[0].unanswered,
        Some(Unanswered::Failed(_))
    ));
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(360.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "send-again-narrow");
    let button = label_at(&t, "Send again").expect("Send again");
    let note = label_where(&t, |s| s.starts_with("Failed: ")).expect("the note");
    assert!(button.height() < 20.0, "one line: {button:?}");
    assert!(
        button.min_y() >= note.max_y(),
        "under the note: {button:?} {note:?}"
    );
}

/// The same over text that selects on its own in the band (the selection
/// shown, a rewrite's preview, a prompt): its drag scrolls the band too.
pub fn a_drag_in_shown_text_past_the_bottom_scrolls_and_selects_on(proxy: &LaunchProxy) {
    let text: String = (1..=80)
        .map(|i| format!("Line {i} of the text.\n"))
        .collect();
    let panel = panel(proxy, "drag-scroll-shown", &[]);
    panel.open(Mode::Rewrite, Some(text.clone()));
    wait_for("the selection", || panel.ui().selection.is_some());
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    let (_, at) = paragraph_with(&t, "Line 1 of").expect("the selection shown");
    let start = (f64::from(at.min_x()) + 2.0, f64::from(at.min_y()) + 5.0);
    t.move_cursor(start);
    t.press_cursor(start);
    settle(&mut t);
    let below = (start.0, f64::from(WINDOW.1) - 10.0); // over the composer
    t.move_cursor(below);
    t.poll(Duration::from_millis(16), Duration::from_millis(3000));
    shot(&mut t, "drag-scroll-shown");
    let (lit, at) = paragraph_with(&t, "Line 80 of").expect("still laid out");
    t.release_cursor(below);
    settle(&mut t);
    assert!(at.max_y() < WINDOW.1, "scrolled to the end: {at:?}");
    let end = lit.iter().map(|&(a, b)| a.max(b)).max().unwrap_or(0);
    assert!(
        end + 30 > text.len(),
        "selected to the end: {end} of {}",
        text.len()
    );
}

/// A replacement opened from History: one drag from the text it replaced
/// into its result selects both and the After heading between them; ⌘C
/// copies them in order.
pub fn a_drag_selects_a_replacements_before_and_after(proxy: &LaunchProxy) {
    use blitznir_core::store::{Made, Store};
    let seed = |s: &Store| {
        let made = Made::new("rewrite", "Translate", None);
        (s.add_replacement("Bonjour tout le monde.", "Hello everyone.", &made))
            .expect("a replacement");
    };
    let platform = fake();
    let cfg = blitznir_core::config::Config::default();
    let panel = panel_seeded(proxy, "before-after", &[], platform.clone(), cfg, seed);
    panel.open_recent(0);
    wait_for("the row open", || panel.ui().about.is_some());
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    let (_, from) = paragraph_with(&t, "Hello everyone.").expect("the original");
    let (_, to) = paragraph_with(&t, "Bonjour tout").expect("the result");
    let start = (f64::from(from.min_x()) + 1.0, f64::from(from.center().y));
    let end = (f64::from(to.max_x()) - 2.0, f64::from(to.center().y));
    t.move_cursor(start);
    t.press_cursor(start);
    settle(&mut t);
    t.move_cursor(end);
    t.release_cursor(end);
    settle(&mut t);
    shot(&mut t, "before-after");
    super::chat::command(&mut t, "c");
    wait_for("the copy", || platform.clipboard().contains("Bonjour"));
    assert_eq!(
        platform.clipboard(),
        "Hello everyone.\n\nAfter\n\nBonjour tout le monde."
    );
}

/// A double click in the chat's text selects the word under it, a triple
/// click its paragraph: here a replacement's result.
pub fn a_double_click_selects_a_word_a_triple_its_paragraph(proxy: &LaunchProxy) {
    use blitznir_core::store::{Made, Store};
    let seed = |s: &Store| {
        let made = Made::new("rewrite", "Translate", None);
        (s.add_replacement("Bonjour tout le monde.", "Hello everyone.", &made))
            .expect("a replacement");
    };
    let platform = fake();
    let cfg = blitznir_core::config::Config::default();
    let panel = panel_seeded(proxy, "double-click", &[], platform.clone(), cfg, seed);
    panel.open_recent(0);
    wait_for("the row open", || panel.ui().about.is_some());
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    let (_, at) = paragraph_with(&t, "Bonjour tout").expect("the result");
    // "Bonjour " is about 55 pt at 13 pt: inside "tout"
    let tout = (f64::from(at.min_x()) + 66.0, f64::from(at.center().y));
    t.move_cursor(tout);
    t.click_cursor(tout);
    settle(&mut t); // the press focuses the transcript, for ⌘C
    t.click_cursor(tout);
    settle(&mut t);
    shot(&mut t, "double-click");
    super::chat::command(&mut t, "c");
    wait_for("the word copied", || !platform.clipboard().is_empty());
    assert_eq!(platform.clipboard(), "tout");
    std::thread::sleep(Duration::from_millis(600)); // the next press starts anew
    for _ in 0..3 {
        t.click_cursor(tout);
    }
    settle(&mut t);
    super::chat::command(&mut t, "c");
    wait_for("the paragraph copied", || platform.clipboard() != "tout");
    assert_eq!(platform.clipboard(), "Bonjour tout le monde.");
}
