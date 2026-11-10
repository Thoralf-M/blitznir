use super::*;

pub(crate) fn prompts_md() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/prompts.md"
    ))
    .unwrap()
}

/// The `nth` fenced block opened by `fence` (e.g. "```text") after `heading`.
pub(crate) fn block(md: &str, heading: &str, fence: &str, nth: usize) -> String {
    let start = md
        .find(heading)
        .unwrap_or_else(|| panic!("no heading {heading}"));
    let mut rest = &md[start..];
    for _ in 0..=nth {
        let open = rest.find(&format!("{fence}\n")).unwrap() + fence.len() + 1;
        rest = &rest[open..];
    }
    let close = fence
        .chars()
        .take_while(|c| *c == fence.as_bytes()[0] as char)
        .collect::<String>();
    rest[..rest.find(&format!("\n{close}\n")).unwrap()].to_string()
}

/// JSON vector lines `[input, expected]` of a block.
pub(crate) fn vectors(block: &str) -> Vec<(String, serde_json::Value)> {
    block
        .lines()
        .map(|l| {
            serde_json::from_str::<(String, serde_json::Value)>(l)
                .unwrap_or_else(|e| panic!("{l}: {e}"))
        })
        .collect()
}

#[test]
fn prompts_are_verbatim() {
    let md = prompts_md();
    for (heading, value) in [
        ("## Chat with a selection", CHAT_SYSTEM_PROMPT),
        ("## Chat without a selection", CHAT_PLAIN_SYSTEM),
        ("## Image rewrite", REWRITE_IMAGE_PROMPT),
        ("## Language check", LANG_DETECT_PROMPT),
        ("## Translate", TRANSLATE_PROMPT),
    ] {
        assert_eq!(value, block(&md, heading, "```text", 0), "{heading}");
    }
    assert_eq!(
        IMAGE_TASK_INSTRUCTION,
        block(&md, "## Image rewrite", "```text", 1)
    );
    assert_eq!(
        IMAGE_TASK_TRANSCRIBE,
        block(&md, "## Image rewrite", "```text", 2)
    );
    let suffix: String =
        serde_json::from_str(&block(&md, "## Image rewrite", "```json", 0)).unwrap();
    assert_eq!(IMAGE_SELECTION_SUFFIX, suffix);
}

#[test]
fn fill_replaces_plainly_and_appends_a_lost_text() {
    assert_eq!(
        fill("a {x} {y} {x} {braces}", &[("x", "1"), ("y", "{x}")]),
        "a 1 {x} 1 {braces}"
    );
    assert_eq!(
        fill("Fix this.", &[("text", "hello")]),
        "Fix this.\n\nTEXT:\nhello"
    );
    assert_eq!(
        fill("Do {instruction}", &[("instruction", "it"), ("text", "t")]),
        "Do it\n\nTEXT:\nt"
    );
    assert_eq!(prompt("   ", "builtin"), "builtin");
    assert_eq!(prompt(" mine {text} ", "builtin"), "mine {text}");
}

#[test]
fn translate_direction() {
    let md = prompts_md();
    let t = Translate::default();
    assert_eq!(
        translate_instruction(true, &t),
        block(&md, "## Translate", "```text", 1)
    );
    assert_eq!(
        translate_instruction(false, &t),
        block(&md, "## Translate", "```text", 2)
    );
    assert!(is_home("Yes.", "German", "anything"));
    assert!(is_home(" ja", "German", "anything"));
    assert!(!is_home("No", "German", "Das ist ein Test"));
    assert!(!is_home("nein.", "German", "Das ist ein Test"));
    // no usable answer: the German heuristic, for a German home only
    assert!(is_home("", "german ", "Das ist ein Test"));
    assert!(!is_home("maybe", "Deutsch", "The quick brown fox"));
    assert!(!is_home("", "French", "Das ist ein Test"));
}

#[test]
fn looks_german_vectors() {
    let md = prompts_md();
    for (text, want) in vectors(&block(&md, "## German heuristic", "```json", 0)) {
        assert_eq!(looks_german(&text), want.as_bool().unwrap(), "{text:?}");
    }
    assert_eq!(GERMAN_WORDS.len(), 58);
    let listed = block(&md, "The function words, sorted", "```text", 0);
    assert_eq!(GERMAN_WORDS.join(" "), listed);
}

#[test]
fn drafters_and_helpers_are_hidden() {
    let md = prompts_md();
    let re = hide_regex(crate::config::HIDE_MODELS).unwrap();
    for (id, want) in vectors(&block(&md, "## Model filter", "```json", 0)) {
        assert_eq!(is_chat_model(&re, &id), want.as_bool().unwrap(), "{id:?}");
    }
    let listed = block(&md, "## Model filter", "```text", 0);
    assert_eq!(crate::config::HIDE_MODELS, listed);
    assert!(!is_chat_model(&re, "gemma-4-12b-it@?"));
    assert!(is_chat_model(&re, "gemma-4-12b-it"));
    assert!(is_chat_model(&re, "gemma-4-e4b-it-mlx@6bit"));
    assert!(is_chat_model(&re, "what?"));
}

#[test]
fn request_bodies_follow_the_table() {
    let cfg = Config::default();
    let text = "äh hallo";
    let b = rewrite_body(&cfg, "m", text, "", None, &[]);
    assert_eq!(
        (b["temperature"].as_f64(), b["top_p"].as_f64()),
        (Some(0.2), Some(0.9))
    );
    assert_eq!(b["max_tokens"], 1024 + 8 + 2048);
    assert_eq!(b["chat_template_kwargs"]["enable_thinking"], false);
    assert!(
        b["messages"][0]["content"]
            .as_str()
            .unwrap()
            .ends_with("TEXT:\näh hallo")
    );
    let b = rewrite_body(&cfg, "m", text, "", Some("shorter"), &[]);
    assert_eq!(
        (b["temperature"].as_f64(), b["max_tokens"].as_u64()),
        (Some(0.4), Some(1024 + 8 + 2048))
    );
    let long = "x".repeat(20_000);
    assert_eq!(
        rewrite_body(&cfg, "m", &long, "", None, &[])["max_tokens"],
        crate::llm::MAX_TOKENS_CEILING
    );
    let img = rewrite_body(
        &cfg,
        "m",
        "",
        "",
        None,
        &["data:image/png;base64,AA".into()],
    );
    assert_eq!(img["max_tokens"], 4096 + 2048);
    assert_eq!(
        img["messages"][0]["content"][1]["image_url"]["url"],
        "data:image/png;base64,AA"
    );
    let lang = lang_check_body("m", "German", &"ö".repeat(3000));
    assert_eq!(
        (lang["temperature"].as_f64(), lang["max_tokens"].as_u64()),
        (Some(0.0), Some(2056))
    );
    assert!(lang.get("top_p").is_none());
    assert!(
        lang["messages"][0]["content"]
            .as_str()
            .unwrap()
            .ends_with(&"ö".repeat(2000))
    );
}

#[test]
fn a_button_runs_its_own_prompt() {
    let cfg = Config::default();
    let asked = |own: &str, instruction: Option<&str>| {
        let b = rewrite_body(&cfg, "m", "the text", own, instruction, &[]);
        b["messages"][0]["content"].as_str().unwrap().to_string()
    };
    let sum = "Summarize in one sentence:\n\n{text}";
    assert_eq!(asked(sum, None), "Summarize in one sentence:\n\nthe text");
    assert_eq!(asked("Summarize.", None), "Summarize.\n\nTEXT:\nthe text");
    // typed text goes where the prompt asks for it, else after it
    assert_eq!(asked("{instruction}: {text}", Some("Do")), "Do: the text");
    assert_eq!(
        asked(sum, Some(" Do ")),
        "Summarize in one sentence:\n\nthe text\n\nINSTRUCTION:\nDo"
    );
    // nothing typed: {instruction} is empty, the prompt says what that means
    let rewrite = asked(REWRITE_PROMPT, None);
    assert!(
        rewrite.ends_with("INSTRUCTION:\n\n\nTEXT:\nthe text"),
        "{rewrite}"
    );
    // "" is the built-in one, which Translate wraps its instruction in
    assert_eq!(
        asked("", Some("Do")),
        fill(
            REWRITE_PROMPT,
            &[("instruction", "Do"), ("text", "the text")]
        )
    );
    let b = rewrite_body(&cfg, "m", "t", sum, None, &[]);
    assert_eq!(
        (b["temperature"].as_f64(), b["max_tokens"].as_u64()),
        (Some(0.2), Some(1024 + 1 + 2048))
    );
    let b = rewrite_body(&cfg, "m", "t", sum, Some("Do"), &[]);
    assert_eq!(b["temperature"].as_f64(), Some(0.4));
}

#[test]
fn an_instruction_on_nothing_writes_from_scratch() {
    let cfg = Config::default();
    // what the panel sends: the Rewrite button's own copy of the prompt
    let button = &cfg.actions[0].prompt;
    assert_eq!(cfg.actions[0].label, "Rewrite");
    let b = rewrite_body(&cfg, "m", " ", button, Some("Schreib eine Geschichte"), &[]);
    let ask = b["messages"][0]["content"].as_str().unwrap();
    assert_eq!(
        ask,
        WRITE_PROMPT.replace("{instruction}", "Schreib eine Geschichte")
    );
    assert_eq!(b["max_tokens"].as_u64(), Some(4096 + 1 + 2048));
    assert_eq!(
        rewrite_prompt(&cfg, " ", "", Some("Do"), false),
        WRITE_PROMPT.replace("{instruction}", "Do")
    );
    let sum = rewrite_body(&cfg, "m", "", "Sum {text}", Some("Do"), &[]);
    let write_do = WRITE_PROMPT.replace("{instruction}", "Do");
    assert_eq!(sum["messages"][0]["content"], write_do.as_str());
    let formal = rewrite_body(&cfg, "m", "", "Make it formal.", Some("Do"), &[]);
    assert_eq!(formal["messages"][0]["content"], write_do.as_str());
    // a prompt built on the instruction alone still runs
    let own = rewrite_body(&cfg, "m", "", "Haiku on {instruction}", Some("Do"), &[]);
    assert!(
        own["messages"][0]["content"]
            .as_str()
            .unwrap()
            .starts_with("Haiku on Do")
    );
}

#[test]
fn the_vocabulary_spellings_precede_every_rewrite() {
    let mut cfg = Config::default();
    let asked = |cfg: &Config, own: &str| {
        let b = rewrite_body(cfg, "m", "o m l x", own, None, &[]);
        b["messages"][0]["content"].as_str().unwrap().to_string()
    };
    assert_eq!(
        asked(&cfg, "Sum {text}"),
        "Sum o m l x",
        "none listed: no line"
    );
    cfg.vocabulary.terms = vec!["PostgreSQL".into()];
    cfg.vocabulary
        .replace
        .insert("o m l x".into(), "oMLX".into());
    let line = "Keep these spellings exactly: PostgreSQL, oMLX.\n\n";
    assert_eq!(asked(&cfg, "Sum {text}"), format!("{line}Sum o m l x"));
}

#[test]
fn chat_body_folds_the_selection_into_the_system_prompt() {
    let mut cfg = Config::default();
    let turns = [Turn {
        role: "user",
        content: "why?",
        images: &[],
    }];
    let b = chat_body(&cfg, "m", "", &turns);
    assert_eq!(b["messages"][0]["content"], CHAT_PLAIN_SYSTEM);
    assert_eq!(b["messages"][1]["content"], "why?");
    assert_eq!(
        (
            b["max_tokens"].as_u64(),
            b["chat_template_kwargs"]["enable_thinking"].as_bool()
        ),
        (Some(MAX_TOKENS_CEILING as u64), Some(false))
    );
    cfg.llm.chat_max_tokens = 32_768;
    assert_eq!(chat_body(&cfg, "m", "", &turns)["max_tokens"], 32_768);
    cfg.llm.chat_reasoning = true;
    let b = chat_body(&cfg, "m", "the text", &turns);
    assert!(
        b["messages"][0]["content"]
            .as_str()
            .unwrap()
            .ends_with("TEXT:\nthe text")
    );
    assert_eq!(b["chat_template_kwargs"]["enable_thinking"], true);
}

#[test]
fn a_settings_edit_sends_every_key_the_file_and_the_instruction() {
    let current = "[hotkeys]\ndouble_tap_ms = 450 # {instruction} stays literal\n";
    let body = settings_body("m", current, " set double_tap_ms to 600 ");
    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs[0]["content"], SETTINGS_PROMPT);
    let user = msgs[1]["content"].as_str().unwrap();
    let (defaults, rest) = user.split_once("CURRENT FILE:\n").unwrap();
    assert!(defaults.contains(DEFAULT_TOML));
    assert!(defaults.contains("# The buttons left of Chat") && defaults.contains("[[actions]]"));
    assert!(
        rest.starts_with(current),
        "no placeholder was filled in the files"
    );
    assert!(rest.ends_with("INSTRUCTION:\nset double_tap_ms to 600"));
    assert_eq!(body["temperature"], 0.2);
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
    let k: Kind = serde_json::from_str("\"settings\"").unwrap();
    assert_eq!(k, Kind::Settings);
}

#[test]
fn the_prompt_kept_is_the_one_sent_with_the_text_left_out() {
    let cfg = Config::default();
    let kept = |own: &str, i: Option<&str>| rewrite_prompt(&cfg, "the text", own, i, false);
    assert_eq!(kept("Sum up.", None), "Sum up.\n\nTEXT:\n{text}");
    assert_eq!(kept("{instruction}: {text}", Some(" Do ")), "Do: {text}");
    assert_eq!(
        kept("", Some("Do")),
        fill(REWRITE_PROMPT, &[("instruction", "Do")])
    );
    assert_eq!(
        kept("", Some(" ")),
        fill(REWRITE_PROMPT, &[("instruction", "")])
    );
    let seen = rewrite_prompt(&cfg, "", "", None, true);
    assert!(seen.starts_with("The attached image(s)") && !seen.contains("{text}"));
}
