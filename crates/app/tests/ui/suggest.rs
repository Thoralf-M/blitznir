//! `[dictation] rewrite_replaces = false`: the dictation stays and the box
//! offers the rewrite; the box's switch writes the setting.

use super::kit::*;
use blitznir_core::platform::Platform;
use blitznir_core::platform::fake::FakePlatform;
use freya::prelude::LaunchProxy;
use std::sync::Arc;

const SAID: &str = "hello there";
const REWRITE: &str = "Hello there.";

fn suggesting(
    proxy: &LaunchProxy,
    name: &str,
    platform: &Arc<FakePlatform>,
) -> Arc<blitznir_app::panel::Panel> {
    let mut cfg = local_config(&answering_endpoint(REWRITE));
    cfg.dictation.rewrite_replaces = false;
    panel_over(proxy, name, &[], platform.clone(), cfg)
}

/// A live dictation stays as typed while the rewrite is only offered; Use
/// rewrite swaps it in.
pub fn a_suggested_rewrite_leaves_the_dictation_until_taken(proxy: &LaunchProxy) {
    let mut fake = FakePlatform::new();
    fake.mouse = Some((400.0, 400.0)); // the box goes above it
    let platform = Arc::new(fake);
    platform.type_text(SAID).expect("typed");
    let panel = suggesting(proxy, "suggest-live", &platform);
    panel.rewrite_dictation(SAID.into(), true);
    let stored = || panel.ui().recent.iter().any(|h| h.text == REWRITE);
    wait_for("the rewrite in the history", stored);
    assert_eq!(platform.screen(), SAID, "the dictation stays");
    let taken = || {
        panel.revert(); // a no-op until the offer is up
        platform.screen() == REWRITE
    };
    wait_for("Use rewrite to swap it in", taken);
}

/// `rewrite_auto = false`: the dictation stays as typed and nothing runs;
/// the box offers Rewrite, which swaps the rewrite in (even in Suggest mode).
pub fn a_dictation_offers_its_rewrite_on_click(proxy: &LaunchProxy) {
    let mut fake = FakePlatform::new();
    fake.mouse = Some((400.0, 400.0));
    let platform = Arc::new(fake);
    platform.type_text(SAID).expect("typed"); // batch types it at once now
    let mut cfg = local_config(&answering_endpoint(REWRITE));
    cfg.dictation.rewrite_auto = false;
    cfg.dictation.rewrite_replaces = false; // the automatic rewrite's only
    let panel = panel_over(proxy, "rewrite-later", &[], platform.clone(), cfg);
    panel.rewrite_dictation(SAID.into(), true);
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(panel.ui().busy.is_none(), "no rewrite runs by itself");
    assert_eq!(platform.screen(), SAID, "the dictation as typed");
    let taken = || {
        panel.rewrite_offered(); // a no-op until the offer is up
        platform.screen() == REWRITE
    };
    wait_for("Rewrite to swap the rewrite in", taken);
    // switched off mid-recording: the batch dictation was held back, so it
    // is rewritten as the recording began, not offered over nothing typed
    let platform = Arc::new(FakePlatform::new());
    let mut cfg = local_config(&answering_endpoint(REWRITE));
    cfg.dictation.rewrite_auto = false;
    let panel = panel_over(proxy, "rewrite-held", &[], platform.clone(), cfg);
    panel.rewrite_dictation(SAID.into(), false);
    wait_for("the held dictation rewritten", || {
        platform.screen() == REWRITE
    });
}

/// A batch dictation, held back while its rewrite ran, is pasted as said;
/// with nowhere to put the box (no bounds, no mouse) the rewrite goes on the
/// clipboard rather than only into the history.
pub fn a_suggested_batch_rewrite_pastes_the_dictation(proxy: &LaunchProxy) {
    let platform = Arc::new(FakePlatform::new());
    let panel = suggesting(proxy, "suggest-batch", &platform);
    panel.rewrite_dictation(SAID.into(), false);
    wait_for("the dictation pasted", || platform.screen() == SAID);
    wait_for("the rewrite on the clipboard", || {
        platform.clipboard() == REWRITE
    });
}

/// A batch dictation's rewrite that fails once the effect's overlay has the
/// key focus (our app in front): what was said still lands where it was
/// dictated, as on Esc.
pub fn a_failed_batch_rewrite_puts_the_dictation_in(proxy: &LaunchProxy) {
    use std::sync::atomic::Ordering;
    let platform = Arc::new(FakePlatform::new());
    let url = failing_endpoint(std::time::Duration::from_millis(500));
    let panel = panel_over(
        proxy,
        "batch-fails",
        &[],
        platform.clone(),
        local_config(&url),
    );
    panel.rewrite_dictation(SAID.into(), false);
    wait_for("the rewrite to run", || panel.ui().busy.is_some());
    platform.ours_in_front.store(true, Ordering::SeqCst);
    wait_for("the rewrite to fail", || panel.ui().busy.is_none());
    wait_for("the dictation pasted", || platform.screen() == SAID);
}

/// The box's Replace / Suggest switch lands in config.toml.
pub fn the_box_switch_writes_the_setting(proxy: &LaunchProxy) {
    let platform = Arc::new(FakePlatform::new());
    let panel = suggesting(proxy, "suggest-switch", &platform);
    let file = std::env::temp_dir()
        .join(format!("blitznir-ui-suggest-switch-{}", std::process::id()))
        .join("config.toml");
    std::fs::write(&file, "[dictation]\nrewrite = \"Fix: {text}\"\n").expect("a config.toml");
    panel.set_rewrite_replaces(false);
    let read = || std::fs::read_to_string(&file).unwrap_or_default();
    wait_for("the switch saved", || {
        read().contains("rewrite_replaces = false")
    });
    assert!(
        read().contains("rewrite = \"Fix: {text}\""),
        "the rest stays"
    );
}

/// The box in both modes, as a frame (`UI_SHOTS`): its text and buttons fit.
pub fn the_box_shows_either_text_and_the_switch(proxy: &LaunchProxy) {
    use blitznir_app::ui::revert::{BoxKind, Shown, app};
    use freya::prelude::Size2D;
    use freya_testing::prelude::*;
    let long = "So the idea is that we ship the dictation rewrite first, then look at the effects, and after that we decide whether the box should stay small for short sentences or grow with the text, because comparing two longer versions in a tiny box means scrolling all the time.";
    let cases = [
        (
            "box-revert",
            BoxKind::Revert,
            SAID,
            (400.0, 132.0),
            "Revert",
        ),
        (
            "box-suggest",
            BoxKind::Suggest,
            REWRITE,
            (400.0, 132.0),
            "Use rewrite",
        ),
        (
            "box-long",
            BoxKind::Suggest,
            long,
            (680.0, 164.0),
            "Use rewrite",
        ),
        ("box-later", BoxKind::Later, SAID, (400.0, 106.0), "Rewrite"),
        (
            "box-translation",
            BoxKind::Translation,
            "Hallo Welt",
            (400.0, 132.0),
            "Open in Blitznir",
        ),
    ];
    for (name, kind, text, (w, h), button) in cases {
        let panel = panel(proxy, name, &[]);
        let shown = Shown {
            text: text.into(),
            kind,
            replaces: match kind {
                BoxKind::Revert => Some(true),
                BoxKind::Suggest => Some(false),
                BoxKind::Translation | BoxKind::Later => None,
            },
        };
        let view = move || app(panel.clone(), shown.clone());
        let (mut t, ()) = TestingRunner::new(view, Size2D::new(w, h), |_| {}, 1.0);
        settle(&mut t);
        shot(&mut t, name);
        let has = |t: &TestingRunner, text: &str| {
            t.find(|_, e| Label::try_downcast(e).filter(|l| l.text.as_ref() == text))
                .is_some()
        };
        assert!(has(&t, button), "{name}: {button}");
        let switch = matches!(kind, BoxKind::Revert | BoxKind::Suggest);
        assert_eq!(
            has(&t, "Next time:"),
            switch,
            "{name}: the Replace/Suggest switch"
        );
        if kind == BoxKind::Suggest {
            press_label(&mut t, "Replace");
            settle(&mut t);
            shot(&mut t, &format!("{name}-replace"));
            let row = label_where(&t, |l| l == "Suggest").expect("the switch stays");
            assert!(row.max_y() <= h, "{name}: the switch in the box");
        }
    }
}

/// An attached image's thumbnail takes the image's proportions: a tall
/// screenshot is a narrow thumbnail with its Remove on its corner, not a
/// sliver at the left of a 160 pt frame with the Remove far beside it.
pub fn a_tall_image_gets_a_narrow_thumbnail(proxy: &LaunchProxy) {
    use blitznir_core::lock;
    use freya::elements::image::Image;
    use freya::prelude::Size2D;
    use freya_testing::prelude::*;
    let mut png = Vec::new();
    image::RgbImage::from_pixel(60, 300, image::Rgb([200, 120, 40]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("a PNG");
    let platform = Arc::new(FakePlatform::new());
    *lock(&platform.image) = Some(png);
    let panel = panel_over(
        proxy,
        "thumb",
        &[],
        platform,
        local_config(&silent_endpoint()),
    );
    panel.paste_image(true);
    wait_for("the image attached", || panel.ui().images.len() == 1);
    let p = panel.clone();
    let app = move || blitznir_app::ui::app(p.clone());
    let (mut t, ()) = TestingRunner::new(app, Size2D::new(WINDOW.0, WINDOW.1), |_| {}, 1.0);
    settle(&mut t);
    shot(&mut t, "thumb-tall");
    let width = t
        .find(|node, e| Image::try_downcast(e).map(|_| node.layout().area.width()))
        .expect("the thumbnail");
    assert!(width <= 45.0, "narrow like the image, not 160 pt: {width}");
}

/// Translate with nothing selected makes what is typed a new chat: the
/// original as the turn, the translation as the answer; the field empties.
pub fn translate_with_nothing_selected_translates_the_field(proxy: &LaunchProxy) {
    use blitznir_app::panel::Mode;
    use blitznir_core::tasks::Kind;
    const SAID: &str = "Ein Nest mit Holzspänen";
    let platform = Arc::new(FakePlatform::new());
    let cfg = local_config(&answering_endpoint(REWRITE));
    let panel = panel_over(proxy, "translate-typed", &[], platform, cfg);
    panel.open(Mode::Rewrite, Some(String::new())); // opened on nothing selected
    wait_for("the open", || panel.ui().selection.as_deref() == Some(""));
    panel.set_composer(SAID.into());
    panel.run(Kind::Translate, 0);
    wait_for("the answer", || {
        panel.ui().messages.len() == 2 && panel.ui().streaming.is_none()
    });
    let ui = panel.ui();
    assert!(ui.chatting, "shown as a chat");
    assert!(
        ui.messages[0].text.contains(SAID),
        "the original stays in sight"
    );
    assert_eq!(ui.messages[1].text, REWRITE, "its translation below");
    assert_eq!(ui.composer.text, "", "the field is empty again");
}
