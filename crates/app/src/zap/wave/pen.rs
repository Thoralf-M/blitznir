//! The wave's pen: dots, hairlines and the pre-made glows, in one of two
//! inks. Light adds up (as light does): it only brightens what is below,
//! the overlay's ink over any app; the dark ink draws dark grey for the
//! panel's light theme. Each call is one path per pass.

use super::Pt;
use super::sprite::{INK, Rgb, Sprite, Sprites};
use freya::engine::prelude::{
    BlendMode, Canvas, Color4f, Colors, FilterMode, Gradient, Interpolation, Paint, PaintCap,
    PaintJoin, PaintStyle, PathBuilder, SkRect, TileMode, shaders,
};

/// How the wave is inked: by the background it is drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    /// Additive light: a dark background.
    Light,
    /// Dark grey: a light background.
    Dark,
}

/// One pass of an ink: its blend, whether it is the dark ink, its share of
/// the alpha and how much wider it draws.
struct Pass {
    blend: BlendMode,
    ink: bool,
    alpha: f32,
    grow: f32,
}

const LIGHT: [Pass; 1] = [Pass {
    blend: BlendMode::Plus,
    ink: false,
    alpha: 1.0,
    grow: 1.0,
}];
const DARK: [Pass; 1] = [Pass {
    blend: BlendMode::SrcOver,
    ink: true,
    alpha: 0.75,
    grow: 1.0,
}];
impl Ink {
    fn passes(self) -> &'static [Pass] {
        match self {
            Ink::Light => &LIGHT,
            Ink::Dark => &DARK,
        }
    }
}

impl Pass {
    fn color(&self, rgb: Rgb, a: f32) -> Color4f {
        let [r, g, b] = if self.ink { INK } else { rgb };
        Color4f::new(r, g, b, (a * self.alpha).clamp(0.0, 1.0))
    }

    fn paint(&self, rgb: Rgb, a: f32) -> Paint {
        let mut p = Paint::default();
        p.set_anti_alias(true)
            .set_blend_mode(self.blend)
            .set_color4f(self.color(rgb, a), None);
        p
    }

    /// A stroke's width: the halo pass a little wider still.
    fn width(&self, w: f32) -> f32 {
        w * self.grow + if self.grow > 1.0 { 0.6 } else { 0.0 }
    }

    /// A glow's size: the halo pass 1.3 times as large.
    fn scale(&self) -> f32 {
        if self.grow > 1.0 { 1.3 } else { 1.0 }
    }
}

/// What a frame draws with, and what it drew.
pub struct Pen<'a> {
    pub c: &'a Canvas,
    path: &'a mut PathBuilder,
    passes: &'static [Pass],
    pub sprites: &'a Sprites,
    /// Particles and draw calls this frame.
    pub dots: usize,
    pub draws: usize,
}

impl<'a> Pen<'a> {
    pub fn new(c: &'a Canvas, path: &'a mut PathBuilder, ink: Ink, sprites: &'a Sprites) -> Self {
        let passes = ink.passes();
        Pen {
            c,
            path,
            passes,
            sprites,
            dots: 0,
            draws: 0,
        }
    }

    /// Many dots of one colour, each `(centre, radius)`.
    pub fn dots(&mut self, pts: &[(Pt, f32)], rgb: Rgb, a: f32) {
        if pts.is_empty() || a <= 0.002 {
            return;
        }
        (self.dots, self.draws) = (self.dots + pts.len(), self.draws + 1);
        for pass in self.passes {
            let add = if pass.grow > 1.0 { 0.35 } else { 0.0 };
            for (p, r) in pts {
                self.path.add_circle(*p, r.max(0.2) * pass.grow + add, None);
            }
            let path = self.path.detach();
            self.c.draw_path(&path, &pass.paint(rgb, a));
        }
    }

    /// Short hairlines, round-capped.
    pub fn segs(&mut self, segs: &[[Pt; 2]], width: f32, rgb: Rgb, a: f32) {
        if segs.is_empty() || a <= 0.002 {
            return;
        }
        self.draws += 1;
        for [p, q] in segs {
            self.path.move_to(*p);
            self.path.line_to(*q);
        }
        let path = self.path.detach();
        for pass in self.passes {
            let mut paint = pass.paint(rgb, a);
            paint
                .set_style(PaintStyle::Stroke)
                .set_stroke_width(pass.width(width))
                .set_stroke_cap(PaintCap::Round);
            self.c.draw_path(&path, &paint);
        }
    }

    /// A polyline whose light fades in over `fade` points at either end of `xa..xb`.
    pub fn stroke_h(&mut self, pts: &[Pt], width: f32, rgb: Rgb, a: f32, (xa, xb): Pt, fade: f32) {
        let Some((first, rest)) = pts.split_first() else {
            return;
        };
        if a <= 0.002 || xb - xa < 1.0 || rest.is_empty() {
            return;
        }
        self.draws += 1;
        self.path.move_to(*first);
        for p in rest {
            self.path.line_to(*p);
        }
        let path = self.path.detach();
        let f = (fade / (xb - xa)).min(0.45);
        for pass in self.passes {
            let colors = [0.0, a, a, 0.0].map(|k| pass.color(rgb, k));
            let stops = [0.0, f, 1.0 - f, 1.0];
            let g = Gradient::new(
                Colors::new(&colors, Some(&stops[..]), TileMode::Clamp, None),
                Interpolation::default(),
            );
            let mut paint = pass.paint(rgb, a);
            paint
                .set_shader(shaders::linear_gradient(((xa, 0.0), (xb, 0.0)), &g, None))
                .set_style(PaintStyle::Stroke)
                .set_stroke_width(pass.width(width))
                .set_stroke_join(PaintJoin::Round);
            self.c.draw_path(&path, &paint);
        }
    }

    fn blit(&self, s: &Sprite, pass: &Pass, r: SkRect, a: f32) {
        let mut paint = pass.paint([1.0; 3], 1.0);
        paint.set_alpha_f((a * pass.alpha).clamp(0.0, 1.0));
        let image = if pass.ink { &s.ink } else { &s.light };
        self.c
            .draw_image_rect_with_sampling_options(image, None, r, FilterMode::Linear, &paint);
    }

    /// Glow `s` stretched over `x, y, w, h`.
    pub fn img(&mut self, s: &Sprite, (x, y, w, h): (f32, f32, f32, f32), a: f32) {
        if a <= 0.002 || w < 0.5 {
            return;
        }
        self.draws += 1;
        for pass in self.passes {
            let g = pass.scale();
            let (x, y) = (x - (g - 1.0) * w / 2.0, y - (g - 1.0) * h / 2.0);
            self.blit(s, pass, SkRect::from_xywh(x, y, w * g, h * g), a);
        }
    }

    /// Glow `s` on every `(centre, radius)`, `k` radii across.
    pub fn imgs(&mut self, s: &Sprite, pts: &[(Pt, f32)], k: f32, a: f32) {
        if pts.is_empty() || a <= 0.002 {
            return;
        }
        self.draws += 1;
        for pass in self.passes {
            for ((x, y), r) in pts {
                let d = r * k * pass.grow;
                self.blit(
                    s,
                    pass,
                    SkRect::from_xywh(x - d / 2.0, y - d / 2.0, d, d),
                    a,
                );
            }
        }
    }

    /// The shimmer band over `xa..xb` round `y`, `hh` high either way, its
    /// ends fading over `fade` points whatever its length.
    pub fn band(&mut self, (xa, xb): Pt, y: f32, hh: f32, a: f32, fade: f32) {
        if a <= 0.002 || xb - xa < 2.0 {
            return;
        }
        self.draws += 1;
        let f = fade.min((xb - xa) / 2.0);
        let [l, m, r] = &self.sprites.band;
        for pass in self.passes {
            let h = hh * 2.0 * pass.scale();
            let top = y - h / 2.0;
            self.blit(l, pass, SkRect::from_xywh(xa, top, f, h), a);
            self.blit(
                m,
                pass,
                SkRect::from_xywh(xa + f, top, xb - xa - 2.0 * f, h),
                a,
            );
            self.blit(r, pass, SkRect::from_xywh(xb - f, top, f, h), a);
        }
    }
}
