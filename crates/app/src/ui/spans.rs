//! A markdown run's look in an answer: headings, bold, links, code.

use super::style::Palette;
use blitznir_core::markdown::Run;
use freya::prelude::*;

fn heading_size(level: u8) -> f32 {
    match level {
        1 => 20.0,
        2 => 18.0,
        3 => 16.0,
        4 => 14.0,
        _ => super::answer::SIZE,
    }
}

pub fn span(p: &Palette, r: &Run, fg: Color) -> Span<'static> {
    let s = &r.style;
    let mut span = Span::new(r.text.clone()).color(if s.link.is_some() { p.accent } else { fg });
    if s.heading > 0 {
        span = span
            .font_size(heading_size(s.heading))
            .font_weight(FontWeight::BOLD);
    }
    if s.bold {
        span = span.font_weight(FontWeight::BOLD);
    }
    if s.italic {
        span = span.font_slant(FontSlant::Italic);
    }
    if s.strike {
        span = span.text_decoration(TextDecoration::LineThrough);
    } else if s.link.is_some() {
        span = span.text_decoration(TextDecoration::Underline);
    }
    if s.code {
        span = span.font_family("monospace").color(p.accent);
    }
    span
}
