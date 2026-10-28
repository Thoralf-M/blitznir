//! `[[actions]]`: the panel's buttons left of Chat, in order, each with its
//! one prompt; `enter` and `chat_enter` mark what Return runs outside and
//! inside a chat. tasks.rs turns a button into a request. `[prompts]`: the
//! chat's system message.

use crate::keys::Key;
use serde::Deserialize;

/// Blank = the built-in prompt (tasks::prompt), as a missing key is.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Prompts {
    pub chat: String,
}

impl Default for Prompts {
    fn default() -> Self {
        Self {
            chat: crate::tasks::CHAT_SYSTEM_PROMPT.into(),
        }
    }
}

/// More do not fit the composer's row next to Translate and Chat.
const MAX: usize = 6;
/// Longer labels are cut on the button anyway; the tooltip has it whole.
const LABEL_MAX: usize = 24;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Button {
    pub label: String,
    /// {text} is the selection, {instruction} what was typed.
    pub prompt: String,
    /// Return runs this one; none marked = Return starts a chat.
    #[serde(default)]
    pub enter: bool,
    /// Return in a chat runs this one; none marked = Return sends.
    #[serde(default)]
    pub chat_enter: bool,
    /// Runs this one on the selection, no panel, as [hotkeys] quick does.
    #[serde(default)]
    pub hotkey: Vec<Key>,
}

/// No [[actions]] in the file: the one button DEFAULT_TOML shows.
pub(super) fn defaults() -> Vec<Button> {
    vec![Button {
        label: "Rewrite".into(),
        prompt: crate::tasks::REWRITE_PROMPT.into(),
        enter: true,
        chat_enter: false,
        hotkey: vec![],
    }]
}

pub(super) fn validate(buttons: &[Button]) -> Result<(), String> {
    if !(1..=MAX).contains(&buttons.len()) {
        return Err(format!("[[actions]]: list 1 to {MAX} buttons"));
    }
    for (i, b) in buttons.iter().enumerate() {
        let n = b.label.trim().chars().count();
        if n == 0 || n > LABEL_MAX {
            return Err(format!(
                "[[actions]] button {}: label must be 1 to {LABEL_MAX} characters",
                i + 1
            ));
        }
        if b.prompt.trim().is_empty() {
            return Err(format!(
                "[[actions]] button {:?}: its prompt is empty",
                b.label
            ));
        }
    }
    let count = |f: fn(&Button) -> bool| buttons.iter().filter(|b| f(b)).count();
    if count(|b| b.enter) > 1 {
        return Err("[[actions]]: enter = true on one button only, the one Return runs".into());
    }
    if count(|b| b.chat_enter) > 1 {
        return Err(
            "[[actions]]: chat_enter = true on one button only, the one Return runs in a chat"
                .into(),
        );
    }
    Ok(())
}
