//! The history's rows: capped per kind, kept with what made them, deleted
//! one at a time or all of a kind.

use super::{
    super::tests::{made, texts, tmp},
    *,
};
use std::os::unix::fs::PermissionsExt;

#[test]
fn history_is_capped_newest_first() {
    let t = tmp(4);
    for i in 1..=6 {
        t.0.add_history(&format!("text {i}"), "live").unwrap();
    }
    assert_eq!(texts(&t.0), ["text 6", "text 5", "text 4", "text 3"]);
    assert_eq!(t.0.history()[0].mode, "live");
}

#[test]
fn history_skips_empty_text_and_a_repeat_of_the_newest() {
    let t = tmp(4);
    t.0.add_history("  ", "live").unwrap();
    t.0.add_history(" one ", "live").unwrap();
    t.0.add_history("one", "recovered").unwrap();
    t.0.add_history("two", "batch").unwrap();
    t.0.add_history("one", "live").unwrap(); // not the newest: moves up, once
    assert_eq!(texts(&t.0), ["one", "two"]);
    assert_eq!(t.0.history()[1].mode, "batch");
}

#[test]
fn replacements_keep_their_original_and_are_capped_apart_from_dictations() {
    let t = tmp(2);
    t.0.add_history("said", "batch").unwrap();
    for i in 1..=3 {
        t.0.add_replacement(
            &format!("fixed {i}"),
            &format!(" fixd {i} "),
            &made("rewrite"),
        )
        .unwrap();
    }
    t.0.add_replacement("Hallo", "", &made("translate"))
        .unwrap();
    // a dictation written out is a replacement too; the dictation stays
    t.0.add_replacement("said", "sel", &made("write-out"))
        .unwrap();
    let h = t.0.history();
    let kinds: Vec<_> = h.iter().map(|i| (i.text.as_str(), i.kind())).collect();
    use HistoryKind::{Dictation, Replacement};
    assert_eq!(
        kinds,
        [
            ("said", Replacement),
            ("Hallo", Replacement),
            ("said", Dictation),
        ],
        "two of each kind at most; the rewrites went, the dictation did not"
    );
    assert_eq!(h[0].original.as_deref(), Some("sel"));
    assert_eq!(h[1].original, None, "an empty selection is no original");
    for mode in ["live", "batch", "recovered"] {
        t.0.add_history(mode, mode).unwrap();
        assert_eq!(t.0.history()[0].kind(), Dictation, "{mode}");
    }
}

#[test]
fn a_history_without_originals_still_reads() {
    let t = tmp(4);
    std::fs::write(
        t.1.history(),
        r#"[{"text":"old","mode":"live","ts":"2026-01-01T10:00:00"}]"#,
    )
    .unwrap();
    assert_eq!(t.0.history()[0].original, None);
    t.0.add_history("new", "live").unwrap();
    let raw = std::fs::read_to_string(t.1.history()).unwrap();
    assert!(!raw.contains("original"), "no null written for a dictation");
}

#[test]
fn rows_are_deleted_one_at_a_time_or_all_of_a_kind() {
    let t = tmp(4);
    t.0.add_history("a", "live").unwrap();
    t.0.add_history("b", "batch").unwrap();
    t.0.add_replacement("B.", "b", &made("rewrite")).unwrap();
    t.0.add_replacement("Hallo", "Hello", &made("translate"))
        .unwrap();
    let b = t.0.history()[2].clone();
    assert_eq!(b.text, "b");
    assert!(t.0.remove_history(&b).unwrap());
    assert!(!t.0.remove_history(&b).unwrap(), "gone already");
    assert_eq!(texts(&t.0), ["Hallo", "B.", "a"]);
    assert!(
        !t.1.dir.join("history.json.tmp").exists(),
        "written atomically"
    );
    assert_eq!(t.0.clear_history(HistoryKind::Replacement).unwrap(), 2);
    assert_eq!(texts(&t.0), ["a"]);
    assert_eq!(t.0.clear_history(HistoryKind::Replacement).unwrap(), 0);
    assert_eq!(t.0.clear_history(HistoryKind::Dictation).unwrap(), 1);
    assert!(t.0.history().is_empty());
    assert_eq!(
        std::fs::metadata(t.1.history())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn a_replacement_keeps_what_made_it_and_old_rows_read_without() {
    let t = tmp(4);
    let old = r#"[{"text":"Old.","mode":"rewrite","ts":"2020-01-02T09:30:00","original":"old"}]"#;
    std::fs::write(t.1.history(), old).unwrap();
    assert_eq!(t.0.history()[0].action, None, "a row from before");
    let made = Made::new("rewrite", "Summarize", Some("Shorter:\n{text}".into()));
    t.0.add_replacement("Short.", "a long text", &made).unwrap();
    t.0.add_history("said", "live").unwrap();
    let h = t.0.history();
    assert_eq!(h[1].action.as_deref(), Some("Summarize"));
    assert_eq!(h[1].prompt.as_deref(), Some("Shorter:\n{text}"));
    assert_eq!(h[1].original.as_deref(), Some("a long text"));
    assert_eq!((h[0].action.clone(), h[0].prompt.clone()), (None, None));
    let json = std::fs::read_to_string(t.1.history()).unwrap();
    assert_eq!(json.matches("\"prompt\"").count(), 1, "None is not written");
}
