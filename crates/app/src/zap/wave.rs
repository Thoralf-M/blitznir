//! The wave: a hairline stream of
//! light with specks flowing toward a soft glowing tip where the text
//! arrives. Drawn in two places: under the caret line of the app being
//! dictated into ([`super::caret`], in light only), and in a chat
//! answer's place until its first word (ui/thinking.rs, in the panel's
//! ink). The timeline and the tip's spring are core's wave; this
//! draws a frame. Never over text: the text's rects are clipped out; only
//! the chat's fading trail crosses the words just written.

mod pen;
mod sprite;
mod stream;

#[cfg(test)]
mod tests;

pub use pen::Ink;
pub use stream::Stream;

use blitznir_core::wave::Heads;
use freya::engine::prelude::{Canvas, ClipOp, PathBuilder, SkRect};
use pen::Pen;
use sprite::Sprites;
use stream::Bufs;

pub type Pt = (f32, f32);

/// The trail over fresh text: where the head was, seconds in, strength.
pub struct Trail<'a> {
    pub heads: &'a Heads,
    pub t: f32,
    pub k: f32,
}

/// What one frame shows.
pub struct Frame<'a> {
    pub streams: &'a [Stream],
    /// The text's rects (x, y, w, h): no stream is drawn over them.
    pub text: &'a [[f32; 4]],
    pub trail: Option<Trail<'a>>,
}

/// The glows made once, and the buffers every frame reuses.
#[derive(Default)]
pub struct Cache {
    sprites: Sprites,
    path: PathBuilder,
    bufs: Bufs,
}

/// What a frame drew: its particles and draw calls.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Drawn {
    pub dots: usize,
    pub draws: usize,
}

/// `f` onto `c` in `ink`.
pub fn draw(c: &Canvas, ink: Ink, cache: &mut Cache, f: &Frame) -> Drawn {
    let Cache {
        sprites,
        path,
        bufs,
    } = cache;
    let mut pen = Pen::new(c, path, ink, sprites);
    if let Some(tr) = &f.trail {
        stream::trail(&mut pen, tr.heads, tr.t, tr.k, bufs);
    }
    c.save();
    for [x, y, w, h] in f.text {
        c.clip_rect(SkRect::from_xywh(*x, *y, *w, *h), ClipOp::Difference, true);
    }
    for s in f.streams {
        stream::stream(&mut pen, s, bufs);
    }
    c.restore();
    Drawn {
        dots: pen.dots,
        draws: pen.draws,
    }
}
