//! Model output → the blocks a native shell draws as selectable text.
//! Paragraphs, headings and lists run together into one [`Block::Text`], so
//! a drag selects across them as in any other app; only what needs its own
//! box (code, quote, table, rule) starts a new block.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    /// Inline `code`.
    pub code: bool,
    /// 1–6 inside a heading, else 0.
    pub heading: u8,
    /// The destination while inside a link.
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub style: Style,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Text(Vec<Run>),
    /// A fenced or indented code block, without its last newline.
    Code(String),
    Quote(Vec<Run>),
    /// Rows of cells; the first row is the header.
    Table(Vec<Vec<Vec<Run>>>),
    Rule,
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    runs: Vec<Run>,
    style: Style,
    /// Open lists: the next number of an ordered one, None for bullets.
    lists: Vec<Option<u64>>,
    /// Some while inside a code block.
    code: Option<String>,
    quote: usize,
    table: Option<Vec<Vec<Vec<Run>>>>,
    /// An item's marker was just written: its first paragraph needs no break.
    item_start: bool,
}

impl Builder {
    fn push(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(code) = &mut self.code {
            return code.push_str(text);
        }
        let style = self.style.clone();
        let runs = match &mut self.table {
            Some(rows) => match rows.last_mut().and_then(|r| r.last_mut()) {
                Some(cell) => cell,
                None => return,
            },
            None => &mut self.runs,
        };
        match runs.last_mut() {
            Some(last) if last.style == style => last.text.push_str(text),
            _ => runs.push(Run {
                text: text.to_string(),
                style,
            }),
        }
        self.item_start = false;
    }

    /// At least `n` newlines before what comes next, none at the start.
    fn gap(&mut self, n: usize) {
        let Some(last) = self.runs.last() else { return };
        let have = last.text.chars().rev().take_while(|&c| c == '\n').count();
        let style = std::mem::take(&mut self.style);
        self.push(&"\n".repeat(n.saturating_sub(have)));
        self.style = style;
    }

    /// Close the running text as a block of its own kind.
    fn flush(&mut self) {
        if let Some(last) = self.runs.last_mut() {
            last.text.truncate(last.text.trim_end_matches('\n').len());
        }
        self.runs.retain(|r| !r.text.is_empty());
        if self.runs.is_empty() {
            return;
        }
        let runs = std::mem::take(&mut self.runs);
        self.blocks.push(if self.quote > 0 {
            Block::Quote(runs)
        } else {
            Block::Text(runs)
        });
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph if self.item_start => {}
            Tag::Paragraph => self.gap(if self.lists.is_empty() { 2 } else { 1 }),
            Tag::Heading { level, .. } => {
                self.gap(2);
                self.style.heading = heading(level);
            }
            Tag::BlockQuote(_) => {
                if self.quote == 0 {
                    self.flush();
                }
                self.quote += 1;
            }
            Tag::CodeBlock(_) => {
                self.flush();
                self.code = Some(String::new());
            }
            Tag::List(start) => {
                self.gap(if self.lists.is_empty() { 2 } else { 1 });
                self.lists.push(start);
            }
            Tag::Item => {
                self.gap(1);
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        *n += 1;
                        format!("{}. ", *n - 1)
                    }
                    _ => "• ".into(),
                };
                self.push(&format!("{}{marker}", "    ".repeat(depth)));
                self.item_start = true;
            }
            Tag::Table(_) => {
                self.flush();
                self.table = Some(vec![]);
            }
            Tag::TableHead | Tag::TableRow => {
                if let Some(rows) = &mut self.table {
                    rows.push(vec![]);
                }
            }
            Tag::TableCell => {
                if let Some(row) = self.table.as_mut().and_then(|t| t.last_mut()) {
                    row.push(vec![]);
                }
            }
            Tag::Emphasis => self.style.italic = true,
            Tag::Strong => self.style.bold = true,
            Tag::Strikethrough => self.style.strike = true,
            Tag::Link { dest_url, .. } => self.style.link = Some(dest_url.into_string()),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => self.style.heading = 0,
            TagEnd::BlockQuote(_) => {
                if self.quote == 1 {
                    self.flush();
                }
                self.quote = self.quote.saturating_sub(1);
            }
            TagEnd::CodeBlock => {
                if let Some(code) = self.code.take() {
                    self.blocks
                        .push(Block::Code(code.trim_end_matches('\n').to_string()));
                }
            }
            TagEnd::List(_) => {
                self.lists.pop();
            }
            TagEnd::Table => {
                if let Some(rows) = self.table.take() {
                    self.blocks.push(Block::Table(rows));
                }
            }
            TagEnd::Emphasis => self.style.italic = false,
            TagEnd::Strong => self.style.bold = false,
            TagEnd::Strikethrough => self.style.strike = false,
            TagEnd::Link => self.style.link = None,
            _ => {}
        }
    }
}

fn heading(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

pub fn blocks(markdown: &str) -> Vec<Block> {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut b = Builder::default();
    for ev in Parser::new_ext(markdown, opts) {
        match ev {
            Event::Start(Tag::Image { dest_url, .. }) => {
                b.push(&format!("[image {dest_url}] "));
            }
            Event::Start(tag) => b.start(tag),
            Event::End(tag) => b.end(tag),
            // raw HTML is shown as the text it is
            Event::Text(t) | Event::Html(t) | Event::InlineHtml(t) => b.push(&t),
            Event::Code(t) => {
                b.style.code = true;
                b.push(&t);
                b.style.code = false;
            }
            Event::SoftBreak => b.push(" "),
            Event::HardBreak => b.push("\n"),
            Event::Rule => {
                b.flush();
                b.blocks.push(Block::Rule);
            }
            Event::TaskListMarker(done) => {
                b.push(if done { "[x] " } else { "[ ] " });
                b.item_start = true;
            }
            _ => {}
        }
    }
    b.flush();
    b.blocks
}

#[cfg(test)]
mod tests;
