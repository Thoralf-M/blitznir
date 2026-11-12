//! Search in config.toml (Settings' text tab): the sidebar's search selects
//! a match in the editor and scrolls it into view; Return goes to the next.

use freya::{code_editor::CodeEditorData, prelude::*, text_edit::TextEditor};

/// The editor's line height: CodeEditor's default 1.4 of the font size.
const LINE: f32 = (super::settings::FONT_SIZE * 1.4) as i32 as f32;

/// Where `query` occurs in `text`, ignoring ASCII case, as byte ranges
/// (ASCII lowering keeps every byte where it was).
pub fn matches(text: &str, query: &str) -> Vec<(usize, usize)> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let hay = text.to_ascii_lowercase();
    hay.match_indices(&q)
        .map(|(i, _)| (i, i + q.len()))
        .collect()
}

/// Select match `n` of `query` (wrapping) and scroll to it: which one it
/// is and how many there are, or None.
pub fn select(
    ed: &mut CodeEditorData,
    query: &str,
    n: usize,
    scroll: ScrollController,
) -> Option<(usize, usize)> {
    let found = matches(&ed.rope.to_string(), query);
    let i = n.checked_rem(found.len())?;
    let (from, to) = found[i];
    let u16 = |byte: usize| ed.rope.char_to_utf16_cu(ed.rope.byte_to_char(byte));
    let range = (u16(from), u16(to));
    ed.set_selection(range);
    // its line a third down the view, from the left edge where it fits
    let mut scroll = scroll;
    let top = ed.cursor_row() as f32 * LINE - ed.viewport.height / 3.0;
    scroll.scroll_to_y(-(top.max(0.0) as i32));
    scroll.scroll_to_x(0);
    ed.scroll_to_cursor(scroll, LINE);
    Some((i, found.len()))
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn finds_every_match_in_any_case() {
        let text = "[panel]\neffect = \"beam\"\n# Effect: beam\n";
        assert_eq!(matches(text, "EFFECT"), vec![(8, 14), (26, 32)]);
        assert!(matches(text, "  ").is_empty());
        assert!(matches(text, "nothing").is_empty());
    }
}
