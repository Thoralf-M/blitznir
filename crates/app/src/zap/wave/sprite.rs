//! The wave's soft glows, each made once as a small image, in its light
//! and in the dark ink, and drawn stretched: nothing is blurred at draw
//! time (a per-frame blur measured 25 times the cost).

use blitznir_core::beam::{ease, smooth};
use freya::engine::prelude::{AlphaType, ColorType, Data, Image, ImageInfo, images};

pub type Rgb = [f32; 3];

/// The dark ink: a light background's particles.
pub const INK: Rgb = [26.0 / 255.0, 29.0 / 255.0, 38.0 / 255.0];

/// One glow in both inks.
pub struct Sprite {
    pub light: Image,
    pub ink: Image,
}

fn gauss(v: f32, s: f32) -> f32 {
    (-(v * v) / (2.0 * s * s)).exp()
}

/// `w` by `h` pixels of `rgb`, each pixel's alpha from its centre (0..1 across and down).
fn image(w: usize, h: usize, rgb: Rgb, alpha: &dyn Fn(f32, f32) -> f32) -> Image {
    let mut px = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let a = alpha((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            px.extend(rgb.map(|c| (c * 255.0).round() as u8));
            px.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    let info = ImageInfo::new(
        (w as i32, h as i32),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    images::raster_from_data(&info, Data::new_copy(&px), w * 4)
        .expect("an RGBA image of its own size")
}

fn sprite(w: usize, h: usize, rgb: Rgb, alpha: impl Fn(f32, f32) -> f32) -> Sprite {
    Sprite {
        light: image(w, h, rgb, &alpha),
        ink: image(w, h, INK, &alpha),
    }
}

/// A 96 px band, fading in over its first 32 px and out over its last.
fn band(x: f32, v: f32) -> f32 {
    let f = if x < 32.0 {
        smooth(x / 32.0)
    } else if x > 64.0 {
        smooth((96.0 - x) / 32.0)
    } else {
        1.0
    };
    f * gauss(v - 0.5, 0.16)
}

/// Every glow the wave draws.
pub struct Sprites {
    /// A round glow: halo dots, and the shimmer's sweep.
    pub dot: Sprite,
    pub flow: Sprite,
    /// The bright point at the tip.
    pub bright: Sprite,
    /// The shimmer band in three slices, so its ends keep their fade at any length.
    pub band: [Sprite; 3],
    /// The head's glow: rises along the tail's ease, narrows to a point at the tip.
    pub head: Sprite,
    /// The core's highlight: brightest just behind the tip, soft on every side.
    pub core: Sprite,
    /// The tip's soft pointed lens.
    pub tip: Sprite,
}

impl Default for Sprites {
    fn default() -> Sprites {
        let cone = |u: f32, v: f32| (1.0 - ((u - 0.5) * 2.0).hypot((v - 0.5) * 2.0)).max(0.0);
        let slice = |i: f32| {
            sprite(32, 32, [0.7, 0.8, 1.0], move |u, v| {
                band(32.0 * i + u * 32.0, v)
            })
        };
        Sprites {
            dot: sprite(32, 32, [0.5, 0.65, 1.0], |u, v| {
                gauss((u - 0.5).hypot(v - 0.5) * 2.0, 0.38)
            }),
            flow: sprite(64, 16, [0.85, 0.9, 1.0], move |u, v| cone(u, v).powf(1.6)),
            bright: sprite(64, 16, [1.0, 1.0, 1.0], move |u, v| {
                0.95 * cone(u, v).powf(1.2)
            }),
            band: [slice(0.0), slice(1.0), slice(2.0)],
            head: sprite(256, 32, [0.6, 0.8, 1.0], |u, v| {
                let tip = 1.0 - ease((u - 0.72) / 0.28);
                let hw = (0.12 + 0.88 * ease(u)) * tip;
                ease(u).powf(1.4) * tip * gauss(v - 0.5, 0.04 + 0.2 * hw)
            }),
            core: sprite(128, 16, [0.95, 0.97, 1.0], |u, v| {
                smooth(u / 0.7) * (1.0 - smooth((u - 0.8) / 0.2)) * gauss(v - 0.5, 0.17)
            }),
            tip: sprite(64, 16, [0.85, 0.92, 1.0], |u, v| {
                let hw = smooth(u / 0.55) * (1.0 - ease((u - 0.6) / 0.4));
                smooth(u / 0.5) * gauss(v - 0.5, 0.03 + 0.22 * hw)
            }),
        }
    }
}
