//! config.toml: the user's file, documented by its own comments. The panel's
//! Settings tab shows it as text and saves it through [`Config::write_text`];
//! a file that does not parse is refused and nothing is written.

mod actions;
pub mod form;
mod linux;
mod paths;
mod rewrite;
mod validate;

use crate::keys::{Action, Key};
use crate::{Error, Result};
pub use actions::{Button, Prompts};
pub use linux::Linux;
pub use paths::{DATA_DIR_NAME, Paths};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub hotkeys: Hotkeys,
    #[serde(default)]
    pub dictation: Dictation,
    #[serde(default)]
    pub vocabulary: Vocabulary,
    #[serde(default)]
    pub translate: Translate,
    #[serde(default)]
    pub llm: Llm,
    /// Absent = none: the default rows live in DEFAULT_TOML, so they can be deleted.
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
    #[serde(default)]
    pub prompts: Prompts,
    #[serde(default = "actions::defaults")]
    pub actions: Vec<Button>,
    #[serde(default)]
    pub capture: Capture,
    #[serde(default)]
    pub panel: Panel,
    #[serde(default)]
    pub linux: Linux,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Hotkeys {
    pub live: Vec<Key>,
    pub batch: Vec<Key>,
    pub rewrite: Vec<Key>,
    pub chat: Vec<Key>,
    pub quick: Vec<Key>,
    pub translate: Vec<Key>,
    pub undo: Vec<Key>,
    pub double_tap_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SttModel {
    Fp32,
    Int8,
}

impl SttModel {
    pub fn as_str(self) -> &'static str {
        match self {
            SttModel::Fp32 => "fp32",
            SttModel::Int8 => "int8",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Dictation {
    pub model: SttModel,
    pub auto_copy: bool,
    pub history_max: usize,
    pub silence_rms: f32,
    pub segment_max_s: f32,
    /// The prompt every finished dictation is rewritten with; "" = off.
    pub rewrite: String,
    /// The rewrite runs as a dictation ends; false (macOS): a box by the
    /// text offers it, so it costs nothing unless asked for.
    pub rewrite_auto: bool,
    /// After a rewrite, the dictation shows by the text with Revert (macOS).
    pub revert: bool,
    /// The rewrite replaces the dictation; false (macOS): the box offers it.
    pub rewrite_replaces: bool,
    /// A dictation into the panel's own composer is rewritten too.
    pub rewrite_in_panel: bool,
}

/// Spoken form → written form, and terms to keep as written (vocab.rs).
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Vocabulary {
    pub replace: BTreeMap<String, String>,
    pub terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Translate {
    pub home: String,
    pub other: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Llm {
    pub chat_reasoning: bool,
    pub chat_max_tokens: usize,
    pub hide_models: String,
    pub vision_models: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub models: Vec<String>,
    /// Whether its models accept images; None = the server and `vision_models` say.
    #[serde(default)]
    pub vision: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Capture {
    pub cmd: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Panel {
    /// The working effect while a rewrite runs (app/zap.rs), or none.
    pub effect: Effect,
    /// The effect's sound volume in % (thunder-synth's thunder for the
    /// lightning, core's beam_sound for the beam); 0 = none, and none without it.
    pub effect_volume: u8,
    /// Rain beside the lightning's thunder, at its volume (thunder-synth).
    pub rain: bool,
    /// While a dictation records, a wave of light under the caret line it types
    /// into (app/zap/caret.rs; macOS) or the panel's composer (app/ui/dictation.rs).
    pub dictation_effect: bool,
    /// A wave of light in a chat answer's place until its first word (app/ui/thinking.rs).
    pub thinking_effect: bool,
}

/// What plays over the text while a replace runs (app/zap.rs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    /// A beam of light from the menu-bar hammer onto the text.
    Beam,
    /// Bolts from it, with thunder.
    Lightning,
    /// None.
    Off,
}

/// The model picker's filter: drafters, embedders, speech models, and `@?$`,
/// the id LM Studio lists an MTP drafter under.
pub const HIDE_MODELS: &str = r"(^|[-_/@])(mtp|draft|drafter|dflash\d*)([-_/@.]|$)|assistant$|(tts|asr|whisper|parakeet|embed)|@\?$";

/// The picker's eye for a model its server does not describe: the usual
/// vision families by name.
pub const VISION_MODELS: &str =
    r"(vl|vision|llava|pixtral|minicpm-v|moondream|smolvlm|idefics|gemma-[34]|multimodal|omni)";

fn keys(names: &[&str]) -> Vec<Key> {
    names
        .iter()
        .map(|n| Key::try_from((*n).to_string()).expect("built-in key name"))
        .collect()
}

impl Default for Hotkeys {
    fn default() -> Self {
        // a MacBook has no right Ctrl, many PC keyboards no right Meta
        let (live, batch): (&[&str], &[&str]) = if cfg!(target_os = "macos") {
            (&["RightAlt", "RightCtrl"], &["RightMeta"])
        } else {
            (&["RightCtrl"], &["RightAlt"])
        };
        Self {
            live: keys(live),
            batch: keys(batch),
            rewrite: keys(&["RightShift"]),
            chat: vec![],
            quick: vec![],
            translate: vec![],
            undo: vec![],
            double_tap_ms: 450,
        }
    }
}

impl Default for Dictation {
    fn default() -> Self {
        Self {
            model: SttModel::Fp32,
            auto_copy: false,
            history_max: 10,
            silence_rms: 0.01,
            segment_max_s: 15.0,
            rewrite: String::new(),
            rewrite_auto: false,
            revert: true,
            rewrite_replaces: true,
            rewrite_in_panel: false,
        }
    }
}

impl Default for Translate {
    fn default() -> Self {
        Self {
            home: "German".into(),
            other: "English".into(),
        }
    }
}

impl Default for Llm {
    fn default() -> Self {
        Self {
            chat_reasoning: false,
            chat_max_tokens: crate::llm::MAX_TOKENS_CEILING,
            hide_models: HIDE_MODELS.into(),
            vision_models: VISION_MODELS.into(),
        }
    }
}

impl Default for Capture {
    fn default() -> Self {
        let cmd = if cfg!(target_os = "macos") {
            "screencapture -i -o -x {path}"
        } else {
            "" // the desktop's screenshot portal: GNOME 50 refuses gnome-screenshot
        };
        Self { cmd: cmd.into() }
    }
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            effect: Effect::Beam,
            effect_volume: 0,
            rain: false,
            dictation_effect: true,
            thinking_effect: true,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::parse(DEFAULT_TOML).expect("DEFAULT_TOML parses")
    }
}

impl Config {
    /// Parse and validate config.toml text. The error names the line.
    pub fn parse(text: &str) -> Result<Config> {
        let cfg: Config = toml::from_str(text).map_err(|e| syntax_error(text, &e))?;
        cfg.validate()
            .map_err(|e| Error::Config(format!("config.toml: {e}")))?;
        Ok(cfg)
    }

    /// The Settings tab's Save: parse `text`, and only if it is valid, write it.
    pub fn write_text(path: &Path, text: &str) -> Result<Config> {
        let cfg = Config::parse(text)?;
        crate::store::write_atomic(path, text.as_bytes())?;
        Ok(cfg)
    }

    /// Read config.toml, writing the commented default first if there is none.
    pub fn load(paths: &Paths) -> Result<Config> {
        let path = paths.config();
        if !path.exists() {
            std::fs::create_dir_all(&paths.dir)?;
            crate::store::write_atomic(&path, DEFAULT_TOML.as_bytes())?;
            log::info!("wrote the default settings to {}", path.display());
        }
        Config::parse(&std::fs::read_to_string(&path)?)
    }

    /// The button Return runs; None = Return starts a chat.
    pub fn enter(&self) -> Option<usize> {
        self.actions.iter().position(|b| b.enter)
    }

    /// The button Return runs in a chat; None = Return sends.
    pub fn chat_enter(&self) -> Option<usize> {
        self.actions.iter().position(|b| b.chat_enter)
    }

    /// The quick hotkey's button: Return's, else the first.
    pub fn quick_action(&self) -> usize {
        self.enter().unwrap_or(0)
    }

    /// Every hotkey with its action, in config order.
    fn hotkeys(&self) -> impl Iterator<Item = (Key, Action)> + '_ {
        let h = &self.hotkeys;
        [
            (&h.live, Action::Live),
            (&h.batch, Action::Batch),
            (&h.rewrite, Action::Rewrite),
            (&h.chat, Action::Chat),
            (&h.quick, Action::Quick),
            (&h.translate, Action::Translate),
            (&h.undo, Action::Undo),
        ]
        .into_iter()
        .flat_map(|(keys, a)| keys.iter().map(move |k| (*k, a)))
        .chain(
            self.actions
                .iter()
                .enumerate()
                .flat_map(|(i, b)| (b.hotkey.iter()).map(move |k| (*k, Action::Button(i)))),
        )
    }

    /// Which key triggers which action.
    pub fn bindings(&self) -> HashMap<Key, Action> {
        self.hotkeys().collect()
    }
}

/// `e` without toml's snippet of the file: line and column (1-based, in
/// characters, as toml counts them) and its message as the detail.
fn syntax_error(text: &str, e: &toml::de::Error) -> Error {
    let at = e.span().map_or(0, |s| s.start);
    let before = text.get(..at).unwrap_or(text);
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    Error::ConfigSyntax {
        line: before.matches('\n').count() + 1,
        column: before[line_start..].chars().count() + 1,
        detail: e.message().trim().to_string(),
    }
}

#[cfg(target_os = "macos")]
macro_rules! os {
    ($mac:literal, $other:literal) => {
        $mac
    };
}
#[cfg(not(target_os = "macos"))]
macro_rules! os {
    ($mac:literal, $other:literal) => {
        $other
    };
}

/// The file written on first start. Every value equals the built-in default.
pub const DEFAULT_TOML: &str = concat!(
    include_str!("config/head.toml"),
    os!(
        r#"live = ["RightAlt", "RightCtrl"]   # text appears while you speak (a MacBook has no right Ctrl)
batch = ["RightMeta"]               # records silently, types everything at the end
"#,
        r#"live = ["RightCtrl"]    # text appears while you speak
batch = ["RightAlt"]     # records silently, types everything at the end
"#
    ),
    include_str!("config/middle.toml"),
    os!(
        "cmd = \"screencapture -i -o -x {path}\"\n",
        "cmd = \"\"   # \"\" = the desktop's own picker (screenshot portal); or a command, e.g. 'grim -g \"$(slurp)\" {path}'\n"
    ),
    include_str!("config/tail.toml"),
);

#[cfg(test)]
mod tests;
