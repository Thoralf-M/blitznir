use super::*;
use serde_json::Value;

fn fixtures() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/textproc.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn cases(name: &str) -> Vec<Value> {
    let all = fixtures();
    let list = all[name].as_array().unwrap().clone();
    assert!(!list.is_empty(), "no {name} fixtures");
    list
}

fn s<'a>(case: &'a Value, key: &str) -> &'a str {
    case[key].as_str().unwrap()
}

/// Every case, all failures reported at once.
fn check(name: &str, f: impl Fn(&Value) -> Value) {
    let failures: Vec<String> = cases(name)
        .iter()
        .filter_map(|c| {
            let got = f(c);
            (got != c["out"]).then(|| format!("{c}\n   got {got}"))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{name}: {} failing\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn tidy_fixtures() {
    check("tidy", |c| tidy(s(c, "in")).into());
}

#[test]
fn smart_join_fixtures() {
    check("smart_join", |c| {
        smart_join(s(c, "prev"), s(c, "new")).into()
    });
}

#[test]
fn stable_prefix_fixtures() {
    check("stable_prefix", |c| {
        stable_prefix(s(c, "prev"), s(c, "cur")).into()
    });
}

#[test]
fn divergence_fixtures() {
    check("divergence", |c| divergence(s(c, "a"), s(c, "b")).into());
}

#[test]
fn typing_diff_backspaces_code_points() {
    assert_eq!(typing_diff("", "Grüße"), (0, "Grüße"));
    assert_eq!(typing_diff("Grüße", "Grüße"), (0, ""));
    assert_eq!(typing_diff("Grüßen Sie", "Grüße aus"), (5, " aus"));
    assert_eq!(typing_diff("hi 𝄞", "hi 𝄢 da"), (1, "𝄢 da"));
    assert_eq!(typing_diff("abc", ""), (3, ""));
}

#[test]
fn join_adds_the_missing_space() {
    assert_eq!(smart_join("hello", "there"), "hello there");
    assert_eq!(smart_join("hello", " there"), "hello there");
    assert_eq!(smart_join("Done", "!?"), "Done");
}

#[test]
fn backspaces_count_what_the_eye_sees() {
    assert_eq!(backspaces("abc"), 3);
    assert_eq!(backspaces("Grüße"), 5);
    assert_eq!(
        backspaces("ok 🇩🇪 👍🏽"),
        6,
        "a flag and a toned thumb are one each"
    );
    assert_eq!(backspaces("e\u{301}"), 1, "e and a combining accent");
}
