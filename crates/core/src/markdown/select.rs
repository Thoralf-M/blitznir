//! One selection over an answer's pieces of text (a shell's paragraphs in
//! document order: text, code, quote, each table cell). Offsets are UTF-16
//! units, as a laid-out paragraph counts them.

use super::{Block, Run};

/// Stands in for an inline element (a link's open button) in a paragraph:
/// Skia counts a placeholder as one unit.
const PLACEHOLDER: char = '\u{FFFC}';

/// A point in the answer: which piece, how far into it.
pub type Point = (usize, usize);

/// What a paragraph of `runs` holds, a [`PLACEHOLDER`] after each link.
fn piece_text(runs: &[Run]) -> String {
    let mut out = String::new();
    for (i, r) in runs.iter().enumerate() {
        out.push_str(&r.text);
        let link = r.style.link.as_ref();
        if link.is_some() && runs.get(i + 1).and_then(|n| n.style.link.as_ref()) != link {
            out.push(PLACEHOLDER);
        }
    }
    out
}

/// A piece's text and what joins it to the one before when copied: a
/// blank line between blocks, a tab between cells, a newline between rows,
/// so a copied table pastes into a spreadsheet as one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub sep: &'static str,
}

/// A chat message as blocks: an answer's markdown, or the user's words as
/// the one literal paragraph they are.
pub fn message(text: &str, markdown: bool) -> Vec<Block> {
    if markdown {
        return super::blocks(text);
    }
    if text.is_empty() {
        return vec![];
    }
    vec![Block::Text(vec![Run {
        text: text.to_string(),
        style: Default::default(),
    }])]
}

/// Every piece of `blocks`, in the order a shell lays them out.
pub fn pieces(blocks: &[Block]) -> Vec<Piece> {
    let mut out = vec![];
    let block = |text| Piece { text, sep: "\n\n" };
    for b in blocks {
        match b {
            Block::Text(r) | Block::Quote(r) => out.push(block(piece_text(r))),
            Block::Code(c) => out.push(block(c.clone())),
            Block::Table(rows) => {
                for (i, row) in rows.iter().enumerate() {
                    for (j, cell) in row.iter().enumerate() {
                        let sep = match (i, j) {
                            (0, 0) => "\n\n",
                            (_, 0) => "\n",
                            _ => "\t",
                        };
                        out.push(Piece {
                            text: piece_text(cell),
                            sep,
                        });
                    }
                }
            }
            Block::Rule => {}
        }
    }
    out
}

pub fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The part of piece `k` (`len` long) inside the selection `a`–`b` (either
/// order). A selection made before the piece shrank (a streamed "**bol"
/// became "bold") may end past it, or start there too.
pub fn range_in(k: usize, len: usize, a: Point, b: Point) -> Option<(usize, usize)> {
    let (from, to) = if a <= b { (a, b) } else { (b, a) };
    if k < from.0 || k > to.0 || from == to {
        return None;
    }
    let start = if k == from.0 { from.1.min(len) } else { 0 };
    let end = if k == to.0 { to.1.min(len) } else { len };
    (start < end).then_some((start, end))
}

/// The selected text, pieces joined by their `sep`, placeholders dropped.
pub fn selected(pieces: &[Piece], a: Point, b: Point) -> String {
    let mut out = String::new();
    for (k, p) in pieces.iter().enumerate() {
        let Some((start, end)) = range_in(k, len16(&p.text), a, b) else {
            continue;
        };
        if !out.is_empty() {
            out.push_str(p.sep);
        }
        let units: Vec<u16> = p.text.encode_utf16().collect();
        let part = String::from_utf16_lossy(&units[start..end]);
        out.push_str(&part.replace(PLACEHOLDER, ""));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::blocks;

    #[test]
    fn a_selection_spans_text_code_and_table() {
        let b = blocks("Intro **bold**\n\n```\nlet x = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |");
        let p = pieces(&b);
        let texts: Vec<&str> = p.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, ["Intro bold", "let x = 1;", "a", "b", "1", "2"]);
        assert_eq!(selected(&p, (0, 6), (1, 5)), "bold\n\nlet x");
        assert_eq!(
            selected(&p, (1, 5), (0, 6)),
            "bold\n\nlet x",
            "either direction"
        );
        assert_eq!(
            selected(&p, (0, 0), (5, 1)),
            "Intro bold\n\nlet x = 1;\n\na\tb\n1\t2"
        );
    }

    #[test]
    fn messages_join_across_the_chat() {
        let mut p = pieces(&message("**no** markdown here", false));
        p.extend(pieces(&message("**yes**", true)));
        assert_eq!(selected(&p, (0, 0), (1, 3)), "**no** markdown here\n\nyes");
        assert!(message("", false).is_empty());
    }

    #[test]
    fn link_buttons_count_but_are_not_copied() {
        let p = pieces(&blocks("see [docs](https://x.io) now"));
        assert_eq!(p[0].text, format!("see docs{PLACEHOLDER} now"));
        assert_eq!(selected(&p, (0, 4), (0, 13)), "docs now");
    }

    #[test]
    fn offsets_are_utf16() {
        let p = vec![Piece {
            text: "a😀b".into(),
            sep: "",
        }];
        assert_eq!(len16(&p[0].text), 4);
        assert_eq!(selected(&p, (0, 1), (0, 3)), "😀");
    }

    #[test]
    fn a_selection_past_a_shrunk_piece_copies_nothing() {
        // selected in "**bold" while it streamed; the closing ** made it "bold"
        let p = vec![Piece {
            text: "bold".into(),
            sep: "",
        }];
        assert_eq!(range_in(0, 4, (0, 5), (0, 6)), None);
        assert_eq!(selected(&p, (0, 5), (0, 6)), "");
        assert_eq!(selected(&p, (0, 2), (0, 6)), "ld");
    }

    #[test]
    fn an_empty_selection_highlights_nothing() {
        assert_eq!(range_in(0, 5, (0, 2), (0, 2)), None);
        assert_eq!(range_in(1, 5, (0, 2), (2, 1)), Some((0, 5)));
        assert_eq!(range_in(3, 5, (0, 2), (2, 1)), None);
    }
}
