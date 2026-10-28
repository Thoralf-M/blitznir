use super::*;
use crate::config::{Config, DEFAULT_TOML};

fn field(table: &str, key: &str) -> Field {
    *super::field(table, key).expect("in the schema")
}

#[test]
fn every_field_reads_from_the_default_file_and_parses() {
    let cfg: toml_edit::DocumentMut = DEFAULT_TOML.parse().unwrap();
    for f in SECTIONS.iter().flat_map(|s| s.fields) {
        assert!(
            cfg.get(f.table).and_then(|t| t.get(f.key)).is_some(),
            "{}.{} is not in DEFAULT_TOML",
            f.table,
            f.key
        );
        let v = value(DEFAULT_TOML, f);
        set(DEFAULT_TOML, f, &v).unwrap_or_else(|e| panic!("{}.{}: {e}", f.table, f.key));
    }
}

#[test]
fn a_set_keeps_the_comments_and_the_line_comment() {
    let ms = field("hotkeys", "double_tap_ms");
    let text = set(DEFAULT_TOML, &ms, "600").unwrap();
    assert!(
        text.contains("double_tap_ms = 600        # max time between the two taps"),
        "{text}"
    );
    assert_eq!(Config::parse(&text).unwrap().hotkeys.double_tap_ms, 600);
    assert_eq!(
        text.lines().count(),
        DEFAULT_TOML.lines().count(),
        "nothing else moved"
    );
}

#[test]
fn a_set_keeps_the_comment_lines_above_the_key() {
    let map = field("vocabulary", "replace");
    let text = set(DEFAULT_TOML, &map, "a = b").unwrap();
    let example =
        "# Example: replace = { \"o m l x\" = \"oMLX\", \"blitz near\" = \"Blitznir\" }\n";
    assert!(
        text.contains(&format!("{example}replace = {{ a = \"b\" }}\n")),
        "{text}"
    );
    assert_eq!(text.lines().count(), DEFAULT_TOML.lines().count());
    // a [vocabulary.replace] of its own stays one, its lines' comments kept
    let own = "[vocabulary.replace]\n# products\n\"o m l x\" = \"oMLX\"  # spaced\n\
               # names\n\"blitz near\" = \"Blitznir\"\n";
    assert_eq!(
        set(own, &map, "o m l x = OMLX\nkay = K").unwrap(),
        "[vocabulary.replace]\n# products\n\"o m l x\" = \"OMLX\"  # spaced\nkay = \"K\"\n"
    );
}

#[test]
fn kinds_round_trip() {
    let keys = field("hotkeys", "chat");
    let text = set(DEFAULT_TOML, &keys, "F5, F6").unwrap();
    assert_eq!(value(&text, &keys), "F5, F6");
    let map = field("vocabulary", "replace");
    let text = set(&text, &map, "o m l x = oMLX\nblitz near = Blitznir\n").unwrap();
    assert_eq!(value(&text, &map), "o m l x = oMLX\nblitz near = Blitznir");
    let effect = field("panel", "effect");
    assert_eq!(value(&text, &effect), "beam");
    let text = set(&text, &effect, "lightning").unwrap();
    let panel = Config::parse(&text).unwrap().panel;
    assert_eq!(panel.effect, crate::config::Effect::Lightning);
    let off = set(&text, &effect, "off").unwrap();
    assert_eq!(
        Config::parse(&off).unwrap().panel.effect,
        crate::config::Effect::Off
    );
    assert!(
        set(&text, &effect, "sparks").is_err(),
        "only the two effects"
    );
    let prompt = field("prompts", "chat");
    let text = set(&text, &prompt, "Line one\nLine two {text}").unwrap();
    assert!(
        text.contains("chat = '''\nLine one\nLine two {text}'''"),
        "a block, not escapes"
    );
    assert_eq!(value(&text, &prompt), "Line one\nLine two {text}");
}

#[test]
fn what_save_refuses_the_form_refuses_and_the_text_stays() {
    let ms = field("hotkeys", "double_tap_ms");
    assert!(
        set(DEFAULT_TOML, &ms, "abc")
            .unwrap_err()
            .contains("whole number")
    );
    assert!(
        set(DEFAULT_TOML, &ms, "5")
            .unwrap_err()
            .contains("100 and 2000")
    );
    let chat = field("hotkeys", "chat");
    assert!(
        set(DEFAULT_TOML, &chat, "RightShift")
            .unwrap_err()
            .contains("twice")
    );
}

fn labels(text: &str) -> Vec<String> {
    rows(text, Rows::Actions)
        .into_iter()
        .map(|r| r[0].clone())
        .collect()
}

#[test]
fn buttons_are_rows_to_add_edit_and_remove() {
    let r = rows(DEFAULT_TOML, Rows::Actions);
    assert_eq!(r.len(), 1);
    assert_eq!(
        (
            r[0][0].as_str(),
            r[0][1].as_str(),
            r[0][2].as_str(),
            r[0][3].as_str()
        ),
        ("Rewrite", crate::tasks::REWRITE_PROMPT, "true", "")
    );
    let text = add_row(DEFAULT_TOML, Rows::Actions).unwrap();
    let text = set_cell(&text, Rows::Actions, 1, &ACTIONS[0], "Shorten").unwrap();
    let text = set_cell(&text, Rows::Actions, 1, &ACTIONS[1], "Shorter:\n{text}").unwrap();
    let cfg = Config::parse(&text).unwrap();
    assert_eq!(cfg.actions[1].label, "Shorten");
    assert_eq!(cfg.actions[1].prompt, "Shorter:\n{text}");
    assert!(text.starts_with(&DEFAULT_TOML[..DEFAULT_TOML.find("[[actions]]").unwrap()]));
    assert!(
        set_cell(&text, Rows::Actions, 1, &ACTIONS[1], " ")
            .unwrap_err()
            .contains("prompt is empty")
    );
    let text = remove_row(&text, Rows::Actions, 0).unwrap();
    assert_eq!(labels(&text), ["Shorten"]);
    assert!(
        remove_row(&text, Rows::Actions, 0)
            .unwrap_err()
            .contains("at least one")
    );
}

#[test]
fn return_and_return_in_chat_are_picked_on_one_row_or_none() {
    let text = add_row(DEFAULT_TOML, Rows::Actions).unwrap();
    let text = pick(&text, Rows::Actions, &ACTIONS[2], Some(1)).unwrap();
    let text = pick(&text, Rows::Actions, &ACTIONS[3], Some(0)).unwrap();
    let cfg = Config::parse(&text).unwrap();
    assert_eq!((cfg.enter(), cfg.chat_enter()), (Some(1), Some(0)));
    let text = pick(&text, Rows::Actions, &ACTIONS[2], None).unwrap();
    let text = pick(&text, Rows::Actions, &ACTIONS[3], None).unwrap();
    let cfg = Config::parse(&text).unwrap();
    assert_eq!(
        (cfg.enter(), cfg.chat_enter()),
        (None, None),
        "chat and send"
    );
    assert!(
        !text.contains("enter = false"),
        "none marked is no key, not false"
    );
}

#[test]
fn a_file_without_actions_keeps_its_rewrite_when_one_is_added() {
    let bare = "[[endpoints]]\nname = \"x\"\nurl = \"http://127.0.0.1:1/v1\"\n";
    assert_eq!(labels(bare), ["Rewrite"]);
    let cfg = Config::parse(&add_row(bare, Rows::Actions).unwrap()).unwrap();
    let labels: Vec<&str> = cfg.actions.iter().map(|b| b.label.as_str()).collect();
    assert_eq!(labels, ["Rewrite", "Summarize"]);
    assert_eq!(cfg.enter(), Some(0), "Rewrite keeps Return");
}

#[test]
fn added_servers_get_names_of_their_own() {
    let text = add_row(DEFAULT_TOML, Rows::Endpoints).unwrap();
    let text = add_row(&text, Rows::Endpoints).unwrap();
    let names: Vec<String> = (Config::parse(&text).unwrap().endpoints)
        .into_iter()
        .map(|e| e.name)
        .skip(3)
        .collect();
    assert_eq!(names, ["New server", "New server 2"]);
    assert!(
        set_cell(&text, Rows::Endpoints, 4, &ENDPOINTS[0], "New server")
            .unwrap_err()
            .contains("listed twice")
    );
}

#[test]
fn endpoints_vision_is_auto_yes_or_no() {
    let text = set_cell(DEFAULT_TOML, Rows::Endpoints, 1, &ENDPOINTS[4], "yes").unwrap();
    assert_eq!(
        Config::parse(&text).unwrap().endpoints[1].vision,
        Some(true)
    );
    assert_eq!(rows(&text, Rows::Endpoints)[1][4], "yes");
    let text = set_cell(&text, Rows::Endpoints, 1, &ENDPOINTS[4], "auto").unwrap();
    assert_eq!(Config::parse(&text).unwrap().endpoints[1].vision, None);
    assert_eq!(rows(&text, Rows::Endpoints)[0][4], "auto");
    let bad = set_cell(&text, Rows::Endpoints, 0, &ENDPOINTS[1], "localhost:1");
    assert!(bad.unwrap_err().contains("http://"));
}

#[test]
fn the_revert_box_switch_lands_in_a_file_without_the_key() {
    let old = "[dictation]\nrewrite = \"Fix: {text}\"\n";
    let text = set(old, &field("dictation", "rewrite_replaces"), "false").unwrap();
    let cfg = Config::parse(&text).unwrap();
    assert!(!cfg.dictation.rewrite_replaces);
    assert_eq!(cfg.dictation.rewrite, "Fix: {text}", "the rest stays");
    assert!(
        Config::parse(old).unwrap().dictation.rewrite_replaces,
        "default"
    );
}

#[test]
fn the_effects_have_a_section_of_their_own() {
    let keys = |title: &str| -> Vec<&str> {
        let s = SECTIONS.iter().find(|s| s.title == title).expect(title);
        s.fields.iter().map(|f| f.key).collect()
    };
    assert_eq!(
        keys("Effects"),
        [
            "effect",
            "effect_volume",
            "rain",
            "dictation_effect",
            "thinking_effect"
        ]
    );
    let other = keys("Other");
    assert!(
        !other.iter().any(|k| k.contains("effect") || *k == "rain"),
        "Other keeps no effect: {other:?}"
    );
}

#[test]
fn a_field_for_the_other_os_is_neither_shown_nor_found() {
    let linux = field("linux", "xkb_layout");
    let mac = field("dictation", "revert");
    assert_eq!(linux.here(), cfg!(target_os = "linux"));
    assert_eq!(mac.here(), cfg!(target_os = "macos"));
    assert!(field("dictation", "model").here(), "most are for both");
    assert_eq!(
        field_matches(&linux, "Other", "keyboard layout"),
        linux.here()
    );
}
