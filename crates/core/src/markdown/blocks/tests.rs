use super::{Block, Run, blocks};

fn text(b: &Block) -> String {
    let runs: &[Run] = match b {
        Block::Text(r) | Block::Quote(r) => r,
        other => panic!("not text: {other:?}"),
    };
    runs.iter().map(|r| r.text.as_str()).collect()
}

#[test]
fn prose_is_one_block_across_paragraphs_headings_and_lists() {
    let b = blocks("# Title\n\nOne\ntwo.\n\n- a\n- b\n  - c\n\n1. x\n2. y\n\nEnd.");
    assert_eq!(b.len(), 1, "{b:?}");
    assert_eq!(
        text(&b[0]),
        "Title\n\nOne two.\n\n• a\n• b\n    • c\n\n1. x\n2. y\n\nEnd."
    );
}

#[test]
fn styles_ride_on_their_runs() {
    let Block::Text(r) = &blocks("# H\n\n**b** *i* ~~s~~ `c` [l](https://x.io)")[0] else {
        panic!()
    };
    let find = |t: &str| r.iter().find(|x| x.text == t).expect(t).style.clone();
    assert_eq!(find("H").heading, 1);
    assert!(find("b").bold && find("i").italic && find("s").strike && find("c").code);
    assert_eq!(find("l").link.as_deref(), Some("https://x.io"));
}

#[test]
fn code_quote_table_and_rule_are_their_own_blocks() {
    let md = "Intro\n\n```rust\nfn main() {}\n```\n\n> quoted\n> more\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---\n\nOutro";
    let b = blocks(md);
    assert_eq!(text(&b[0]), "Intro");
    assert_eq!(b[1], Block::Code("fn main() {}".into()));
    assert!(matches!(&b[2], Block::Quote(_)) && text(&b[2]) == "quoted more");
    let Block::Table(rows) = &b[3] else {
        panic!("{:?}", b[3])
    };
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|c| c.iter().map(|x| x.text.clone()).collect())
                .collect()
        })
        .collect();
    assert_eq!(cells, [["a", "b"], ["1", "2"]]);
    assert_eq!(b[4], Block::Rule);
    assert_eq!(text(&b[5]), "Outro");
}

#[test]
fn loose_lists_and_tasks_keep_markers_on_their_line() {
    let b = blocks("- [x] done\n- [ ] todo\n\n* one\n\n* two");
    assert_eq!(text(&b[0]), "• [x] done\n• [ ] todo\n\n• one\n• two");
}

#[test]
fn raw_html_and_images_stay_text() {
    let b = blocks("<b>hi</b> ![cat](http://t/c.png)");
    assert_eq!(text(&b[0]), "<b>hi</b> [image http://t/c.png] cat");
}

#[test]
fn a_streaming_half_fence_does_not_lose_text() {
    let b = blocks("Here:\n\n```py\nprint(1)");
    assert_eq!(b[1], Block::Code("print(1)".into()));
}
