use super::*;
use std::os::unix::fs::PermissionsExt;

fn tmp() -> Paths {
    let dir =
        std::env::temp_dir().join(format!("blitznir-config-{}", uuid::Uuid::new_v4().simple()));
    Paths::new(dir)
}

fn err(text: &str) -> String {
    Config::parse(text).unwrap_err().detailed()
}

#[test]
fn default_toml_parses_to_the_built_in_defaults() {
    let cfg = Config::parse(DEFAULT_TOML).unwrap();
    let empty = Config::parse("").unwrap();
    assert_eq!(cfg.hotkeys, Hotkeys::default());
    assert_eq!(cfg.dictation, Dictation::default());
    assert_eq!(cfg.translate, Translate::default());
    assert_eq!(cfg.llm, Llm::default());
    assert_eq!(cfg.prompts, Prompts::default());
    // the file shows the built-in prompts byte for byte; blank still means built-in
    use crate::tasks::{CHAT_SYSTEM_PROMPT, REWRITE_PROMPT, prompt};
    assert_eq!(cfg.prompts.chat, CHAT_SYSTEM_PROMPT);
    assert_eq!(cfg.actions[0].prompt, REWRITE_PROMPT);
    assert_eq!((cfg.enter(), cfg.chat_enter()), (Some(0), None));
    let blank = Config::parse("[prompts]\nchat = \"\"\n").unwrap().prompts;
    assert_eq!(prompt(&blank.chat, CHAT_SYSTEM_PROMPT), CHAT_SYSTEM_PROMPT);
    assert_eq!(cfg.capture, Capture::default());
    assert_eq!(cfg.panel, Panel::default());
    assert_eq!(cfg.panel.effect, Effect::Beam);
    assert_eq!(cfg.panel.effect, Effect::Beam);
    assert_eq!(cfg.linux, Linux::default());
    assert_eq!(cfg.vocabulary, Vocabulary::default());
    assert!(cfg.hotkeys.quick.is_empty() && cfg.hotkeys.undo.is_empty());
    assert_eq!(cfg.actions, actions::defaults());
    assert_eq!(
        Config {
            endpoints: vec![],
            ..cfg.clone()
        },
        empty
    );
    let names: Vec<&str> = cfg.endpoints.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["LM Studio", "Ollama", "oMLX"]);
    assert_eq!(cfg.dictation.model, SttModel::Fp32);
    assert_eq!(cfg.llm.hide_models, HIDE_MODELS);
}

#[test]
fn unknown_keys_are_an_error() {
    assert!(err("[dictation]\nautocopy = true\n").contains("unknown field `autocopy`"));
    assert!(err("[dictaton]\n").contains("unknown field `dictaton`"));
    assert!(
        err("[[endpoints]]\nname = \"x\"\nurl = \"http://a/v1\"\napi_key = \"k\"\n")
            .contains("api_key")
    );
}

#[test]
fn a_broken_toml_is_refused_with_a_message() {
    let msg = err("[dictation]\nauto_copy = tru\n");
    assert!(
        msg.starts_with("config.toml does not parse at line 2, column 13: "),
        "{msg}"
    );
    assert!(err("[hotkeys]\nlive = [\"RightOption\"]\n").contains("unknown key \"RightOption\""));
    assert!(err("[dictation]\nmodel = \"fp16\"\n").contains("fp16"));
}

#[test]
fn the_log_line_of_a_broken_toml_never_quotes_the_file() {
    for (text, line, column) in [
        ("[[endpoints]]\nname = \"x\"\nkey = \"sk-secret\n", 3, 17),
        ("[[endpoints]]\nname = \"x\"\nkey = sk-secret\n", 3, 7),
        ("[hotkeys]\ndouble_tap_ms = \"sk-secret\"\n", 2, 17),
    ] {
        let e = Config::parse(text).unwrap_err();
        let logged = e.to_string();
        assert_eq!(
            logged,
            format!("config.toml does not parse at line {line}, column {column}")
        );
        assert!(e.detailed().len() > logged.len(), "the panel says more");
    }
}

#[test]
fn values_are_validated() {
    assert!(err("[hotkeys]\nlive = [\"F13\"]\nchat = [\"F13\"]\n").contains("F13 is listed twice"));
    assert!(err("[llm]\nhide_models = \"(\"\n").contains("llm.hide_models"));
    assert!(err("[dictation]\nsegment_max_s = 2.0\n").contains("segment_max_s"));
    assert!(err("[capture]\ncmd = \"shoot\"\n").contains("{path}"));
    assert!(err("[panel]\neffect_volume = 101\n").contains("panel.effect_volume"));
    assert!(err("[llm]\nchat_max_tokens = 512\n").contains("llm.chat_max_tokens"));
    assert!(err("[translate]\nhome = \"\"\n").contains("translate.home"));
    assert!(
        err("[panel]\neffect = \"sparks\"\n")
            .contains("expected one of `beam`, `lightning`, `off`")
    );
    assert!(
        err("[panel]\neffect_volume = true\n").contains("expected u8"),
        "a volume"
    );
    assert!(
        err("[hotkeys]\nquick = [\"F16\"]\nundo = [\"F16\"]\n").contains("F16 is listed twice")
    );
    assert!(err("[vocabulary]\nreplace = { \" \" = \"x\" }\n").contains("spoken form is empty"));
    assert!(err("[[endpoints]]\nname = \"x\"\nurl = \"127.0.0.1:1234\"\n").contains("http://"));
    // the picker tells servers by name: the second "x" was never asked
    let one = "[[endpoints]]\nname = \"x\"\nurl = \"http://127.0.0.1:1/v1\"\n";
    assert!(err(&one.repeat(2)).contains("\"x\" is listed twice"));
}

#[test]
fn actions_are_one_to_six_labelled_buttons() {
    let two = "[[actions]]\nlabel = \"Rewrite\"\nprompt = \"R {text}\"\n[[actions]]\nlabel = \"Sum\"\nprompt = \"S {text}\"\n";
    let labels: Vec<String> = Config::parse(two)
        .unwrap()
        .actions
        .into_iter()
        .map(|a| a.label)
        .collect();
    assert_eq!(labels, ["Rewrite", "Sum"]);
    assert!(err("actions = []\n").contains("1 to 6"));
    assert!(err(&"[[actions]]\nlabel = \"x\"\nprompt = \"p\"\n".repeat(7)).contains("1 to 6"));
    assert!(err("[[actions]]\nlabel = \" \"\nprompt = \"p\"\n").contains("button 1: label"));
    assert!(
        err(&format!(
            "[[actions]]\nlabel = \"{}\"\nprompt = \"p\"\n",
            "x".repeat(25)
        ))
        .contains("24")
    );
    assert!(err("[[actions]]\nprompt = \"{text}\"\n").contains("label"));
    // no {text} is no error: the text is appended, as for every prompt
    assert!(Config::parse("[[actions]]\nlabel = \"x\"\nprompt = \"Fix it.\"\n").is_ok());
}

#[test]
fn write_text_refuses_broken_text_and_keeps_the_file() {
    let paths = tmp();
    let cfg = Config::load(&paths).unwrap();
    assert_eq!(cfg, Config::default());
    let before = std::fs::read_to_string(paths.config()).unwrap();
    assert_eq!(before, DEFAULT_TOML);
    assert!(Config::write_text(&paths.config(), "[dictation\n").is_err());
    assert_eq!(std::fs::read_to_string(paths.config()).unwrap(), before);

    let edited = before.replace("auto_copy = false", "auto_copy = true");
    let saved = Config::write_text(&paths.config(), &edited).unwrap();
    assert!(saved.dictation.auto_copy);
    assert!(Config::load(&paths).unwrap().dictation.auto_copy);
    std::fs::remove_dir_all(&paths.dir).unwrap();
}

#[test]
fn the_file_is_private() {
    let paths = tmp();
    Config::load(&paths).unwrap();
    let mode = std::fs::metadata(paths.config())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    std::fs::remove_dir_all(&paths.dir).unwrap();
}

#[test]
fn bindings_map_keys_to_actions() {
    let cfg =
        Config::parse("[hotkeys]\nlive = [\"RightAlt\", \"F13\"]\nbatch = []\nrewrite = []\n")
            .unwrap();
    let b = cfg.bindings();
    assert_eq!(b.len(), 2);
    assert_eq!(b[&Key::F(13)], Action::Live);
    assert_eq!(b[&Key::RightAlt], Action::Live);
    let cfg = Config::parse("[hotkeys]\nquick = [\"F16\"]\nundo = [\"LeftCtrl\"]\n").unwrap();
    let b = cfg.bindings();
    assert_eq!(
        (b[&Key::F(16)], b[&Key::LeftCtrl]),
        (Action::Quick, Action::Undo)
    );
    let cfg = Config::parse("[hotkeys]\ntranslate = [\"F17\"]\n").unwrap();
    assert_eq!(cfg.bindings()[&Key::F(17)], Action::Translate);
    let twice = Config::parse("[hotkeys]\ntranslate = [\"F17\"]\nquick = [\"F17\"]\n");
    assert!(twice.unwrap_err().to_string().contains("listed twice"));
    assert!(
        Config::default().hotkeys.translate.is_empty(),
        "off by default"
    );
}

#[test]
fn a_buttons_hotkey_runs_that_button() {
    let two = "[[actions]]\nlabel = \"Fix\"\nprompt = \"Fix {text}\"\n\
               [[actions]]\nlabel = \"Short\"\nprompt = \"Shorten {text}\"\nhotkey = [\"F14\"]\n";
    let cfg = Config::parse(two).unwrap();
    assert_eq!(cfg.bindings()[&Key::F(14)], Action::Button(1));
    assert!(cfg.actions[0].hotkey.is_empty(), "none by default");
    let clash = format!("[hotkeys]\nquick = [\"F14\"]\n{two}");
    let e = Config::parse(&clash).unwrap_err().to_string();
    assert!(e.contains("listed twice"), "{e}");
}

#[test]
fn the_vocabulary_example_parses() {
    let text = "[vocabulary]\nreplace = { \"o m l x\" = \"oMLX\", \"blitz near\" = \"Blitznir\" }\n\
                terms = [\"PostgreSQL\"]\n";
    let v = Config::parse(text).unwrap().vocabulary;
    assert_eq!(v.replace["blitz near"], "Blitznir");
    assert_eq!(v.terms, ["PostgreSQL"]);
    assert!(err("[vocabulary]\nreplace = [\"x\"]\n").contains("expected a map"));
}

#[test]
fn the_data_dir_is_blitznir_or_the_override() {
    let default = Paths::from_env(None).unwrap();
    assert!(default.dir.ends_with(DATA_DIR_NAME), "{:?}", default.dir);
    assert_eq!(Paths::from_env(Some("".into())).unwrap().dir, default.dir);
    let custom = Paths::from_env(Some("/tmp/b z".into())).unwrap();
    assert_eq!(custom.dir, std::path::PathBuf::from("/tmp/b z"));
}

#[test]
fn an_empty_capture_cmd_is_the_portal_on_linux_only() {
    let parsed = Config::parse("[capture]\ncmd = \"\"\n");
    assert_eq!(parsed.is_ok(), cfg!(target_os = "linux"), "{parsed:?}");
}

#[test]
fn return_runs_one_marked_button_or_the_chat() {
    let with = |extra: &str| {
        let mut t = DEFAULT_TOML.replace("enter = true\n", "");
        t.push_str(extra);
        Config::parse(&t)
    };
    let two = "\n[[actions]]\nlabel = \"Sum\"\nprompt = \"Sum {text}\"\nchat_enter = true\n";
    let cfg = with(two).unwrap();
    assert_eq!(
        (cfg.enter(), cfg.chat_enter(), cfg.quick_action()),
        (None, Some(1), 0)
    );
    let both = "\n[[actions]]\nlabel = \"A\"\nprompt = \"a\"\nenter = true\n[[actions]]\nlabel = \"B\"\nprompt = \"b\"\nenter = true\n";
    assert!(
        with(both)
            .unwrap_err()
            .to_string()
            .contains("one button only")
    );
    let empty = "\n[[actions]]\nlabel = \"A\"\nprompt = \" \"\n";
    assert!(
        with(empty)
            .unwrap_err()
            .to_string()
            .contains("prompt is empty")
    );
}

#[test]
fn a_panel_table_without_an_effect_gets_the_beam_and_off_is_a_choice() {
    // a file without `effect`: the beam, its sound kept
    let no_effect = Config::parse("[panel]\neffect_volume = 70\n").unwrap();
    assert_eq!(
        (no_effect.panel.effect, no_effect.panel.effect_volume),
        (Effect::Beam, 70)
    );
    let bolts = Config::parse("[panel]\neffect = \"lightning\"\n").unwrap();
    assert_eq!(bolts.panel.effect, Effect::Lightning);
    let off = Config::parse("[panel]\neffect = \"off\"\n").unwrap();
    assert_eq!(off.panel.effect, Effect::Off, "no effect at all");
}

#[test]
fn rain_is_off_unless_asked_for() {
    // rain is an extra over the thunder, never on by itself
    assert!(!Config::default().panel.rain);
    let old = Config::parse("[panel]\neffect_volume = 70\n").unwrap();
    assert!(!old.panel.rain, "a file from before the key has no rain");
    assert!(Config::parse("[panel]\nrain = true\n").unwrap().panel.rain);
}
