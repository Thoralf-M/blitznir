//! The wave drawn headless into Skia surfaces: frames to look at
//! (`UI_SHOTS=<dir>` saves them), and the cost of a frame.
#![allow(clippy::print_stdout)] // a test's report is its output

use super::*;
use blitznir_core::wave::{LEAD, PAST_CARET};
use freya::engine::prelude::{
    Color, EncodedImageFormat, Font, FontMgr, FontStyle, Paint, Surface, raster_n32_premul,
};
use std::time::Instant;

/// The dictation wave's tip core light, as zap/caret/window.rs draws it.
const CARET_CORE: f32 = 1.35;
/// The stream's own formulas: up to 64 specks (fewer on a short line), about
/// 100 grains at the head, 4 sprays; the count is bounded by construction.
const BUDGET: usize = 160;
const DARK_BG: Color = Color::from_rgb(0x1e, 0x1f, 0x22);

fn surface(w: i32, h: i32, bg: Color) -> Surface {
    let mut s = raster_n32_premul((w, h)).expect("a raster surface");
    s.canvas().clear(bg);
    s
}

fn save(s: &mut Surface, name: &str) {
    let Ok(dir) = std::env::var("UI_SHOTS") else {
        return;
    };
    let image = s.image_snapshot();
    let png = image.encode(None, EncodedImageFormat::PNG, None);
    let png = png.expect("a PNG");
    std::fs::write(
        std::path::Path::new(&dir).join(format!("{name}.png")),
        png.as_bytes(),
    )
    .expect("the frame saved");
}

fn text(c: &Canvas, s: &str, at: Pt, size: f32, color: Color) -> f32 {
    let face = FontMgr::default().legacy_make_typeface(None, FontStyle::normal());
    let font = Font::from_typeface(face.expect("a system typeface"), size);
    let mut p = Paint::default();
    p.set_anti_alias(true).set_color(color);
    c.draw_str(s, at, &font, &p);
    font.measure_str(s, Some(&p)).0
}

fn dictation(t: f32, xb: f32, y: f32) -> Stream {
    Stream {
        t,
        xa: xb - 400.0,
        xb,
        y,
        k: 0.9,
        core: CARET_CORE,
    }
}

/// The tip four times as large, to judge its detail.
#[test]
fn the_dictation_wave_tip_up_close() {
    let mut cache = Cache::default();
    for (name, bg, ink) in [
        ("dark", DARK_BG, Ink::Light),
        ("light-ink", Color::WHITE, Ink::Dark),
    ] {
        for t in [2.4, 2.9] {
            let mut s = surface(800, 200, bg);
            s.canvas().scale((4.0, 4.0));
            let streams = [dictation(t, 152.0, 23.5)];
            let f = Frame {
                streams: &streams,
                text: &[],
                trail: None,
            };
            let drawn = draw(s.canvas(), ink, &mut cache, &f);
            assert!(drawn.dots > 30 && drawn.dots <= BUDGET, "{drawn:?}");
            save(&mut s, &format!("dictation-tip4x-{name}-t{t}"));
        }
    }
}

/// A line dictated into: the stream 5 points under the baseline, the text
/// clipped out, the tip just right of the caret.
#[test]
fn the_dictation_wave_under_a_line() {
    let mut cache = Cache::default();
    for (name, bg, fg) in [
        ("dark", DARK_BG, Color::from_rgb(0xe4, 0xe4, 0xe6)),
        ("light", Color::WHITE, Color::from_rgb(0x1d, 0x1d, 0x1f)),
    ] {
        let mut s = surface(1120, 224, bg);
        let c = s.canvas();
        c.scale((2.0, 2.0));
        let (x0, base) = (28.0, 74.0);
        let w = text(c, "Let's ship the beta on Friday,", (x0, base), 15.0, fg);
        let caret = x0 + w;
        let streams = [Stream {
            xa: x0 - 6.0,
            ..dictation(2.4, caret + PAST_CARET, base + 5.0)
        }];
        let clip = [[x0 - 3.0, base - 15.0, w + 5.0, 18.6]];
        let f = Frame {
            streams: &streams,
            text: &clip,
            trail: None,
        };
        draw(c, Ink::Light, &mut cache, &f);
        save(&mut s, &format!("dictation-line-{name}"));
    }
}

/// A chat answer streaming: through the middle of its line, the tip ahead
/// of the text, a trail over the words just written.
#[test]
fn the_chat_wave_through_a_streaming_line() {
    let mut cache = Cache::default();
    for (name, bg, fg, ink) in [
        (
            "dark",
            Color::from_rgb(0x1c, 0x1d, 0x21),
            Color::from_rgb(0xe6, 0xe6, 0xe8),
            Ink::Light,
        ),
        (
            "light",
            Color::WHITE,
            Color::from_rgb(0x1d, 0x1d, 0x1f),
            Ink::Dark,
        ),
    ] {
        let mut s = surface(1120, 200, bg);
        let c = s.canvas();
        c.scale((2.0, 2.0));
        let (x0, base) = (36.0, 50.0);
        text(
            c,
            "The beta ships Friday after the last QA pass, feedback",
            (x0, base),
            14.0,
            fg,
        );
        let w = text(
            c,
            "is collected over the weekend in",
            (x0, base + 21.0),
            14.0,
            fg,
        );
        let (head, mid) = (x0 + w, base + 21.0 - 5.0);
        let mut heads = Heads::default();
        for i in 0..8 {
            let t = 2.0 + i as f32 * 0.07;
            let x = head - 70.0 + i as f32 * 10.0;
            heads.push(t, (x, mid), x);
        }
        let streams = [Stream {
            t: 2.5,
            xa: x0 - 6.0,
            xb: head + LEAD,
            y: mid,
            k: 1.0,
            core: 1.0,
        }];
        let clip = [
            [x0 - 2.0, base - 14.0, 420.0, 19.0],
            [x0 - 2.0, base + 21.0 - 14.0, w + 4.0, 19.0],
        ];
        let trail = Some(Trail {
            heads: &heads,
            t: 2.5,
            k: 1.0,
        });
        let f = Frame {
            streams: &streams,
            text: &clip,
            trail,
        };
        let drawn = draw(c, ink, &mut cache, &f);
        assert!(drawn.dots < 200, "{drawn:?}");
        save(&mut s, &format!("thinking-line-{name}"));
    }
}

/// A frame's cost: 96 frames 0.1 s apart onto a
/// 560 × 112 line at Retina scale, in every ink.
#[test]
fn a_frame_costs_little() {
    let mut cache = Cache::default();
    let mut s = surface(1120, 224, DARK_BG);
    s.canvas().scale((2.0, 2.0));
    for ink in [Ink::Light, Ink::Dark] {
        let t0 = Instant::now();
        let mut peak = Drawn::default();
        for i in 0..96 {
            let streams = [dictation(0.05 + i as f32 * 0.1, 300.0, 79.0)];
            let f = Frame {
                streams: &streams,
                text: &[],
                trail: None,
            };
            let d = draw(s.canvas(), ink, &mut cache, &f);
            peak = Drawn {
                dots: peak.dots.max(d.dots),
                draws: peak.draws.max(d.draws),
            };
        }
        s.image_snapshot(); // the raster work is done by now
        let ms = t0.elapsed().as_secs_f64() * 1000.0 / 96.0;
        println!(
            "wave frame {ink:?}: {ms:.2} ms, peak {} dots, {} draws",
            peak.dots, peak.draws
        );
        assert!(peak.dots <= BUDGET, "the particle budget: {peak:?}");
        assert!(ms < 25.0, "{ink:?}: {ms:.2} ms a frame");
    }
}
