//! The dictation wave's window drawn headless: a caret moving word by word
//! along a line, the frames saved with `UI_SHOTS=<dir>`, and their cost.
#![allow(clippy::print_stdout)] // a test's report is its output

use super::Scene;
use super::window::{Tip, draw};
use crate::zap::wave::{Cache, Ink};
use blitznir_core::platform::Rect;
use blitznir_core::wave::spot;
use freya::engine::prelude::{
    Color, EncodedImageFormat, Paint, SkRect, Surface, raster_n32_premul,
};
use std::time::{Duration, Instant};

const LINE: Rect = Rect {
    x: 400.0,
    y: 300.0,
    w: 120.0,
    h: 18.0,
};

fn scene(start: Instant, line: Rect) -> Scene {
    let at = spot(&line);
    Scene {
        id: 1,
        start,
        end: None,
        spot: Some(at),
        frame: at.frame,
        moved: start,
    }
}

fn frame_png(s: &mut Surface, name: &str) {
    if let Ok(dir) = std::env::var("UI_SHOTS") {
        let image = s.image_snapshot();
        let png = image
            .encode(None, EncodedImageFormat::PNG, None)
            .expect("a PNG");
        let path = std::path::Path::new(&dir).join(format!("{name}.png"));
        std::fs::write(path, png.as_bytes()).expect("saved");
    }
}

/// Words land every 0.4 s; the frames at 30 a second follow them. Over a
/// light and a dark app (the dual ink), with the text as grey blocks.
#[test]
fn the_wave_follows_the_caret_word_by_word_and_fades_out() {
    let t0 = Instant::now();
    let widest = Rect {
        w: LINE.w + 160.0,
        ..LINE
    };
    let at = spot(&widest).frame;
    for (name, bg, fg) in [
        ("light", Color::WHITE, Color::from_rgb(0x1d, 0x1d, 0x1f)),
        (
            "dark",
            Color::from_rgb(0x1e, 0x1f, 0x22),
            Color::from_rgb(0xe4, 0xe4, 0xe6),
        ),
    ] {
        let (mut tip, mut cache) = (Tip::default(), Cache::default());
        let mut s: Surface = raster_n32_premul((at.w as i32 * 2, at.h as i32 * 2)).expect("raster");
        let mut spent = Duration::ZERO;
        let mut frames = 0;
        for i in 0..90 {
            let now = t0 + Duration::from_millis(33 * i);
            let words = (i / 12).min(4) as f64;
            let line = Rect {
                w: LINE.w + 40.0 * words,
                ..LINE
            };
            let mut sc = scene(t0, line);
            sc.frame = at; // the window as wide as the last word needs
            if i >= 75 {
                sc.end = Some(t0 + Duration::from_millis(33 * 75));
            }
            let c = s.canvas();
            c.clear(bg);
            c.save();
            c.scale((2.0, 2.0));
            let mut p = Paint::default();
            p.set_color(fg);
            let text = SkRect::from_xywh(
                (line.x - at.x) as f32,
                (line.y - at.y) as f32 + 3.0,
                line.w as f32,
                line.h as f32 - 6.0,
            );
            c.draw_rect(text, &p);
            let t = Instant::now();
            draw(c, &sc, now, &mut tip, &mut cache, Ink::Light);
            spent += t.elapsed();
            frames += 1;
            c.restore();
            if [20, 40, 74, 89].contains(&i) {
                frame_png(&mut s, &format!("dictation-window-{name}-{i}"));
            }
        }
        let ms = spent.as_secs_f64() * 1000.0 / f64::from(frames);
        println!("dictation wave frame ({name}): {ms:.2} ms");
        assert!(ms < 25.0, "{ms:.2} ms a frame");
    }
}

#[test]
fn a_scene_is_gone_once_faded_after_its_end() {
    let t0 = Instant::now();
    let mut s = scene(t0, LINE);
    assert!(!s.gone(t0 + Duration::from_secs(60)), "while it records");
    s.end = Some(t0);
    assert!(!s.gone(t0 + Duration::from_millis(200)));
    assert!(s.gone(t0 + Duration::from_millis(300)));
}
