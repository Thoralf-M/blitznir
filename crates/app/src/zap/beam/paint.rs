//! The beam's pens: strokes, dots and fills in additive light (they add
//! up where they cross, as light does), soft radial blobs, and the buffers
//! every frame reuses, so a frame allocates next to nothing.

use super::Pt;
use freya::engine::prelude::{
    BlendMode, BlurStyle, Canvas, Color4f, Colors, Gradient, Interpolation, MaskFilter, Paint,
    PaintCap, PaintStyle, PathBuilder, SkPath, SkRect, TileMode, shaders,
};

pub fn c4(r: f32, g: f32, b: f32, a: f32) -> Color4f {
    Color4f::new(r, g, b, a.clamp(0.0, 1.0))
}

fn light(color: Color4f, sigma: f32) -> Paint {
    let mut p = Paint::default();
    p.set_anti_alias(true)
        .set_blend_mode(BlendMode::Plus)
        .set_color4f(color, None);
    if sigma > 0.0 {
        p.set_mask_filter(MaskFilter::blur(BlurStyle::Normal, sigma, None));
    }
    p
}

/// A soft radial blob's colours from its centre out, as (stop, colour) at
/// full strength: the blob's strength is the paint's alpha.
pub type Stops = [(f32, Color4f)];

/// A unit radial gradient of `stops`, made once and scaled onto each blob.
pub fn radial(stops: &Stops) -> Paint {
    let colors: Vec<Color4f> = stops.iter().map(|s| s.1).collect();
    let pos: Vec<f32> = stops.iter().map(|s| s.0).collect();
    let colors = Colors::new(&colors, Some(&pos), TileMode::Clamp, None);
    let g = Gradient::new(colors, Interpolation::default());
    let mut p = Paint::default();
    p.set_anti_alias(true).set_blend_mode(BlendMode::Plus);
    p.set_shader(shaders::radial_gradient(((0.0, 0.0), 1.0), &g, None));
    p
}

/// What one frame draws with: a canvas and the reused path and buffers.
pub struct Pen<'a> {
    pub c: &'a Canvas,
    path: &'a mut PathBuilder,
}

/// The buffers the frames share; nothing in them outlives a frame.
#[derive(Default)]
pub struct Buffers {
    pub dots: [Vec<(Pt, f32)>; 4],
    pub segs: [Vec<[Pt; 2]>; 3],
    pub lines: Vec<Vec<Pt>>,
}

impl<'a> Pen<'a> {
    pub fn new(c: &'a Canvas, path: &'a mut PathBuilder) -> Pen<'a> {
        Pen { c, path }
    }

    fn take(&mut self) -> SkPath {
        self.path.detach()
    }

    /// Two-point `segs` stroked `width` wide, blurred by `sigma` (0: sharp).
    pub fn segs(&mut self, segs: &[[Pt; 2]], width: f32, color: Color4f, sigma: f32) {
        if color.a <= 0.001 || segs.is_empty() {
            return;
        }
        for [a, b] in segs {
            self.path.move_to(*a);
            self.path.line_to(*b);
        }
        let path = self.take();
        self.c.draw_path(&path, &stroke(width, color, sigma));
    }

    /// Polylines, as [`Pen::segs`].
    pub fn lines(&mut self, lines: &[Vec<Pt>], width: f32, color: Color4f, sigma: f32) {
        if color.a <= 0.001 || lines.is_empty() {
            return;
        }
        for pts in lines {
            if let Some((first, rest)) = pts.split_first() {
                self.path.move_to(*first);
                for p in rest {
                    self.path.line_to(*p);
                }
            }
        }
        let path = self.take();
        self.c.draw_path(&path, &stroke(width, color, sigma));
    }

    /// Many dots of one colour and blur in one path: cheap particles.
    pub fn dots(&mut self, pts: &[(Pt, f32)], color: Color4f, sigma: f32) {
        if pts.is_empty() || color.a <= 0.001 {
            return;
        }
        for (p, r) in pts {
            self.path.add_circle(*p, r.max(0.2), None);
        }
        let path = self.take();
        self.c.draw_path(&path, &light(color, sigma));
    }

    /// The path built with `build`, filled.
    pub fn fill(&mut self, build: impl FnOnce(&mut PathBuilder), color: Color4f, sigma: f32) {
        build(self.path);
        let path = self.take();
        self.c.draw_path(&path, &light(color, sigma));
    }

    /// A blob of `paint` ([`radial`]) at `p`, stretched to `rx` by `ry`,
    /// at `strength`.
    pub fn blob(&mut self, paint: &mut Paint, (x, y): Pt, (rx, ry): Pt, strength: f32) {
        if strength <= 0.001 {
            return;
        }
        paint.set_alpha_f(strength.min(1.0));
        self.c.save();
        self.c.translate((x, y));
        self.c.scale((rx.max(0.01), ry.max(0.01)));
        self.c.draw_rect(SkRect::new(-1.0, -1.0, 1.0, 1.0), paint);
        self.c.restore();
    }
}

fn stroke(width: f32, color: Color4f, sigma: f32) -> Paint {
    let mut p = light(color, sigma);
    p.set_style(PaintStyle::Stroke)
        .set_stroke_width(width)
        .set_stroke_cap(PaintCap::Round);
    p
}
