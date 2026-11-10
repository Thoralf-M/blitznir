//! A chat's last answer asked again (Retry), and a chat's image copied and
//! shown large as attached, not as downscaled for the model.

use super::kit::*;
use blitznir_app::panel::Mode;
use blitznir_app::ui;
use blitznir_core::config::Paths;
use blitznir_core::lock;
use blitznir_core::platform::fake::FakePlatform;
use blitznir_core::store::{Chat, Store};
use freya::elements::image::Image;
use freya::prelude::{Area, LaunchProxy, Size2D};
use freya_testing::prelude::*;
use std::sync::Arc;

const RETRY: &str = "Retry: delete this answer and ask again";

/// The icon on the row at `y`, right of `x`, whose tooltip reads `tip`.
fn icon_with_tip(t: &mut TestingRunner, (x, y): (f32, f32), tip: &str) -> Option<(f64, f64)> {
    for dx in (4..160).step_by(8) {
        let at = (f64::from(x) + f64::from(dx), f64::from(y));
        t.move_cursor(at);
        animate(t, 700); // the tooltip's delay
        if label_at(t, tip).is_some() {
            return Some(at);
        }
    }
    None
}

/// The "Assistant" labels, top to bottom.
fn assistants(t: &TestingRunner) -> Vec<Area> {
    let mut all = t.find_many(|node, e| {
        Label::try_downcast(e)
            .filter(|l| l.text == "Assistant")
            .map(|_| node.layout().area)
    });
    all.sort_by(|a, b| a.min_y().total_cmp(&b.min_y()));
    all
}

/// A translation shown as a chat turn has no Retry: asked again as a chat
/// turn, it would not be translated by Translate's prompt.
pub fn a_translation_has_no_retry(proxy: &LaunchProxy) {
    let mut chat = Chat::new();
    chat.messages = vec![
        message(
            "user",
            blitznir_core::tasks::translation_turn("Guten Morgen"),
        ),
        message("assistant", "Good morning".into()),
    ];
    let cfg = local_config(&answering_endpoint("Hello."));
    let panel = panel_over(
        proxy,
        "retry-translation",
        std::slice::from_ref(&chat),
        fake(),
        cfg,
    );
    panel.open(Mode::Chat, Some(String::new()));
    wait_for("the open", || panel.ui().shown);
    panel.open_chat(&chat.id);
    panel.retry(1);
    assert!(panel.ui().busy.is_none(), "nothing asked");
    assert_eq!(panel.ui().messages.len(), 2);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    let row = assistants(&t)[0];
    let at = (row.max_x(), row.center().y);
    assert_eq!(icon_with_tip(&mut t, at, RETRY), None, "no Retry shown");
}

/// Two answered turns: only the last answer has a Retry, and it replaces
/// that answer with a new one, in the window and in chats.json.
pub fn retry_replaces_the_last_answer(proxy: &LaunchProxy) {
    let mut chat = Chat::new();
    chat.name = "Retry".into();
    chat.messages = vec![
        message("user", "First question?".into()),
        message("assistant", "First answer.".into()),
        message("user", "Second question?".into()),
        message("assistant", "An answer to retry.".into()),
    ];
    let cfg = local_config(&answering_endpoint("A better answer."));
    let panel = panel_over(proxy, "retry", std::slice::from_ref(&chat), fake(), cfg);
    panel.open(Mode::Chat, Some(String::new()));
    wait_for("the open", || panel.ui().shown);
    panel.open_chat(&chat.id);
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "retry-before");
    let rows = assistants(&t);
    let row = |a: &Area| (a.max_x(), a.center().y);
    assert_eq!(
        icon_with_tip(&mut t, row(&rows[0]), RETRY),
        None,
        "not on an earlier answer"
    );
    let retry = icon_with_tip(&mut t, row(&rows[1]), RETRY).expect("Retry on the last answer");
    t.click_cursor(retry);
    wait_for("the new answer", || {
        let ui = panel.ui();
        ui.busy.is_none()
            && ui
                .messages
                .last()
                .is_some_and(|m| m.text == "A better answer.")
    });
    settle(&mut t);
    shot(&mut t, "retry-after");
    let texts: Vec<String> = panel.ui().messages.iter().map(|m| m.text.clone()).collect();
    assert_eq!(
        texts,
        [
            "First question?",
            "First answer.",
            "Second question?",
            "A better answer."
        ]
    );
    let stored = Store::new(Paths::new(data_dir("retry")), 10).expect("the store");
    let stored = stored.chat(&chat.id).expect("still stored");
    let said: Vec<&str> = stored.messages.iter().map(|m| m.content.as_str()).collect();
    assert_eq!(
        said, texts,
        "chats.json has the new answer, not the old one"
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

/// An image larger than what the model gets (1280 px): its chat thumbnail
/// copies the original, and shows it large: fitted, 1:1, zoomed, dragged,
/// copied (still open), then closed by a press beside it and by Close.
pub fn a_chat_image_copies_and_shows_as_attached(proxy: &LaunchProxy) {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(2000, 1000, image::Rgb([200, 120, 40]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("a PNG");
    let platform = Arc::new(FakePlatform::new());
    *lock(&platform.image) = Some(png.clone());
    let cfg = local_config(&answering_endpoint("A wide orange image."));
    let panel = panel_over(proxy, "image-copy", &[], platform.clone(), cfg);
    panel.open(Mode::Chat, Some(String::new()));
    wait_for("the open", || panel.ui().shown);
    panel.paste_image(true);
    wait_for("the image attached", || panel.ui().images.len() == 1);
    panel.set_composer("What is this?".into());
    panel.send();
    wait_for("the answer", || {
        let ui = panel.ui();
        ui.busy.is_none() && ui.messages.len() == 2
    });
    let originals = std::fs::read_dir(data_dir("image-copy").join("images"))
        .expect("images/")
        .filter(|e| {
            e.as_ref()
                .is_ok_and(|e| e.path().extension().is_some_and(|x| x == "png"))
        })
        .count();
    assert_eq!(originals, 1, "the original is kept beside what was sent");
    *lock(&platform.image) = None;
    let p = panel.clone();
    let app = move || ui::app(p.clone());
    // larger than the downscaled image: only the original fills it
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(1800.0, 1100.0), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "image-copy-chat");
    let thumb = images(&t)[0];
    let corner = (
        f64::from(thumb.max_x()) - 12.0,
        f64::from(thumb.min_y()) + 12.0,
    );
    t.move_cursor(corner);
    t.click_cursor(corner);
    wait_for("the copy", || lock(&platform.image).is_some());
    assert_eq!(
        lock(&platform.image).as_deref(),
        Some(&png[..]),
        "the original copied"
    );
    *lock(&platform.image) = None;
    settle(&mut t);
    t.click_cursor((f64::from(thumb.min_x()) + 10.0, f64::from(thumb.center().y)));
    settle(&mut t);
    shot(&mut t, "image-copy-large");
    the_viewer_zooms_drags_copies_and_closes(&mut t, &platform, &png);
    t.click_cursor((f64::from(thumb.min_x()) + 10.0, f64::from(thumb.center().y)));
    settle(&mut t);
    press_label(&mut t, "Close");
    assert_eq!(images(&t).len(), 1, "Close closed it");
}

/// The image's size as shown.
fn size(t: &TestingRunner) -> (f32, f32) {
    let a = images(t)[0];
    (a.width().round(), a.height().round())
}

/// The viewer over [`a_chat_image_copies_and_shows_as_attached`]'s
/// 2000×1000 `png` in an 1800×1100 window.
fn the_viewer_zooms_drags_copies_and_closes(
    t: &mut TestingRunner,
    platform: &FakePlatform,
    png: &[u8],
) {
    assert_eq!(
        size(t),
        (1800.0, 900.0),
        "the original fitted to the window, not the 1280 px copy"
    );
    press_label(t, "1:1");
    assert_eq!(size(t), (2000.0, 1000.0), "pixel for pixel");
    press_label(t, "+");
    assert_eq!(size(t), (2500.0, 1250.0));
    // a drag moves it, within its edges: 350 pt to spare across, 75 down
    let before = images(t)[0];
    let from = (900.0, 600.0);
    t.move_cursor(from);
    t.press_cursor(from);
    settle(t);
    t.move_cursor((700.0, 550.0));
    settle(t);
    t.release_cursor((700.0, 550.0));
    settle(t);
    shot(t, "image-copy-dragged");
    let after = images(t)[0];
    assert_eq!(
        (
            after.min_x() - before.min_x(),
            after.min_y() - before.min_y()
        ),
        (-200.0, -50.0),
        "moved with the pointer"
    );
    assert_eq!(
        size(t),
        (2500.0, 1250.0),
        "the press on it did not close it"
    );
    // the very next frame: an image decoded anew per size showed a spinner
    t.scroll((900.0, 600.0), (0.0, 200.0));
    t.sync_and_update();
    assert!(size(t).0 > 2500.0, "the wheel zoomed in: {:?}", size(t));
    press_label(t, "Fit");
    assert_eq!(size(t), (1800.0, 900.0));
    // in the 1066 pt below the bar's 34 pt strip, not in the whole window
    assert_eq!(images(t)[0].center().y, 567.0, "fitted below the bar");
    let bar = label_where(t, |l| l == "Close").expect("the bar");
    assert!(images(t)[0].min_y() > bar.max_y(), "not under the bar");
    press_label(t, "90%"); // the bar, not a button
    assert_eq!(size(t), (1800.0, 900.0), "a press on the bar left it open");
    // a drag from the image let go below it
    t.move_cursor((900.0, 600.0));
    t.press_cursor((900.0, 600.0));
    settle(t);
    t.move_cursor((900.0, 1060.0));
    settle(t);
    t.release_cursor((900.0, 1060.0));
    settle(t);
    assert_eq!(
        size(t),
        (1800.0, 900.0),
        "a drag let go beside it left it open"
    );
    press_label(t, "Copy");
    wait_for("the viewer's copy", || lock(&platform.image).is_some());
    assert_eq!(lock(&platform.image).as_deref(), Some(png));
    assert_eq!(size(t), (1800.0, 900.0), "Copy left it open");
    t.click_cursor((10.0, 1090.0)); // beside it
    settle(t);
    assert_eq!(images(t).len(), 1, "a press beside it closed it");
}
