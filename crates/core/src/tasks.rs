//! Prompts and the request body of every LLM task, the language check that
//! picks the translate direction, and the model picker's filter. Prompts live
//! here only; `fixtures/prompts.md` holds the golden copies tests check them against.

use crate::config::{Config, DEFAULT_TOML, Translate};
use crate::llm::MAX_TOKENS_CEILING;
use regex::{Regex, RegexBuilder};
use serde_json::{Value, json};

mod flow;
mod german;
mod title;
pub use flow::{
    IMAGE_QUESTION, Kind, STILL_COPYING, cannot_run, retryable, translate_pastes, translates_typed,
    translation_turn,
};
#[cfg(test)]
use german::GERMAN_WORDS;
pub use german::looks_german;
pub use title::{TITLE_MAX, TITLE_PROMPT, clean_title, title_body};

/// Room reserved for a reasoning preamble, on top of what the answer needs.
/// Many models reason before answering and cannot be talked out of it — LM
/// Studio's MLX engine ignores `enable_thinking: false` — and they spend that
/// budget in `reasoning_content`. If max_tokens only covers the answer, the run
/// ends mid-thought and `content` comes back EMPTY: measured, only 1
/// of 10 reasoning models could produce a cleanup (675-token budget), while 10
/// of 10 managed it at 4096. Nearly free: max_tokens is a cap, not a target.
pub const REASONING_HEADROOM: usize = 2048;

/// The Rewrite button's prompt, and what Translate wraps its instruction
/// in: with an instruction, follow it; without one, only clean up. One
/// prompt for both cases, so a button has one prompt to read and edit.
pub const REWRITE_PROMPT: &str = "\
The text below was dictated: raw speech-to-text output. It may carry \
dictation artifacts: filler words (uh, um, äh, hmm), false starts, stuttered \
or duplicated words, and words the recognizer misheard so they do not fit the \
sentence. The instruction was dictated too: read it for intent, and never \
treat it as part of the text.

If there is an instruction, follow it fully, however big the change (shorten, \
expand, make it more formal, turn it into bullet points, translate, \
restructure), keep the author's language unless it asks otherwise, and \
silently remove the dictation artifacts from the result.

If the instruction is empty, only clean the text up so it reads as text the \
author could send:
- Remove fillers, stutters, false starts and accidental repetitions.
- Replace a misrecognized word only when the intended word is unambiguous from \
the sentence around it. If you are not sure, leave the word exactly as it is — \
guessing a plausible-sounding replacement is worse than leaving it.
- Fix punctuation, capitalization and obvious grammar slips.
- Keep the author's own wording, tone, structure and language (English stays \
English, German stays German). Do NOT rephrase, reorder, summarize, expand, \
translate or add anything of your own.
- The result must be about as long as the input, minus the removed artifacts.

Do not add commentary, notes, headings or options. Output ONLY the resulting \
text — no preamble, no quotes, no explanation.

INSTRUCTION:
{instruction}

TEXT:
{text}";

/// A Rewrite with an instruction and nothing selected: write from scratch.
/// Not [`REWRITE_PROMPT`] with an empty text: that clean-up frame made Sage
/// hand a German instruction back as the "cleaned" text ("Schreib eine
/// Geschichte": 4 of 4 echoed, while "Write a story" got a story); this one
/// wrote in the instruction's language 6 of 6 times.
pub const WRITE_PROMPT: &str = "\
The instruction below was dictated: raw speech-to-text output, so it may carry \
filler words or misheard words; read it for intent. Nothing is selected, so \
write what it asks for from scratch, in full. Write in the instruction's \
language unless it asks for another.

Output ONLY the resulting text — no preamble, no quotes, no explanation.

INSTRUCTION:
{instruction}";

pub const CHAT_SYSTEM_PROMPT: &str = "\
You are a helpful assistant. The user is working on the text shown below. \
Answer their questions about it — explain, clarify, shorten, reword, translate, \
debug, or discuss it as they ask. Keep answers concise and in the same language \
as the user's message. Do not re-emit the whole text unless explicitly asked.

TEXT:
{text}";

/// The chat's system message when nothing is selected (not overridable).
pub const CHAT_PLAIN_SYSTEM: &str = "You are a helpful assistant. Answer the user's questions concisely, in the same language they use.";

/// A rewrite whose input is (or includes) a picture of text.
pub const REWRITE_IMAGE_PROMPT: &str = "\
The attached image(s) contain text — a screenshot, photo or scan. Read that \
text carefully. {task}
Output ONLY the resulting text, as plain text: no commentary, no notes, no \
headings, no options, and do not describe the image.";
const IMAGE_TASK_INSTRUCTION: &str = "Then follow this instruction on it: \"{instruction}\".";
const IMAGE_TASK_TRANSCRIBE: &str = "Transcribe it faithfully, silently fixing dictation/OCR artifacts, punctuation and obvious typos — nothing else.";
const IMAGE_SELECTION_SUFFIX: &str =
    "\n\nTreat this selected text as further input, along with the image(s):\n";

/// One cheap call decides the translate direction BEFORE translating: told
/// "translate to the other language", a small model hands German back unchanged.
pub const LANG_DETECT_PROMPT: &str = "\
Answer with exactly one word: is the following text written in {lang}? \
Answer \"yes\" or \"no\".

TEXT:
{text}";

pub const TRANSLATE_PROMPT: &str = "Translate the text from {src} into {dst}. Preserve the meaning, tone, \
formatting and level of formality. Output only the {dst} translation, nothing else.";

/// The Settings editor's instruction. The user message holds the default
/// file (every key, documented), the current file and the instruction.
pub const SETTINGS_PROMPT: &str = "\
You edit config.toml, the settings file of a dictation app. It is TOML.
Change the CURRENT FILE exactly as the INSTRUCTION says and nothing else: keep \
every other line, comment and the order as they are. Use only keys and values \
that the DEFAULT FILE documents in its comments; never invent a key.
Output ONLY the complete new file as plain TOML: no code fences, no commentary.";

/// Before a clean-up or an instruction, when [vocabulary] lists any.
const SPELLINGS: &str = "Keep these spellings exactly: {list}.";

/// Fill `{name}` placeholders by plain replacement, in order (a user's prompt
/// may contain other braces). A prompt that lost its {text} gets the text
/// appended, so the model never works blind.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (name, value) in values {
        let key = format!("{{{name}}}");
        if *name == "text" && !out.contains(&key) {
            out.push_str("\n\nTEXT:\n{text}");
        }
        out = out.replace(&key, value);
    }
    out
}

/// The user's prompt, or the built-in one when theirs is blank.
pub fn prompt<'a>(user: &'a str, builtin: &'a str) -> &'a str {
    if user.trim().is_empty() {
        builtin
    } else {
        user.trim()
    }
}

/// `ask` after the [`SPELLINGS`] line, if [vocabulary] has any.
fn keep_spellings(cfg: &Config, ask: String) -> String {
    let keep = crate::vocab::spellings(&cfg.vocabulary);
    if keep.is_empty() {
        return ask;
    }
    format!("{}\n\n{ask}", SPELLINGS.replace("{list}", &keep.join(", ")))
}

fn budget(base: usize, text: &str) -> usize {
    MAX_TOKENS_CEILING.min(base + text.chars().count() + REASONING_HEADROOM)
}

/// What a rewrite asks and its answer's base budget: a button's `own`
/// prompt ("" = [`REWRITE_PROMPT`], what Translate wraps), or read the
/// image(s). What was typed goes into {instruction}, or after a prompt
/// without one.
fn rewrite_ask(
    cfg: &Config,
    text: &str,
    own: &str,
    instruction: Option<&str>,
    images: bool,
) -> (String, usize) {
    let own = own.trim();
    if images {
        let task = instruction.map_or(IMAGE_TASK_TRANSCRIBE.to_string(), |i| {
            IMAGE_TASK_INSTRUCTION.replace("{instruction}", i)
        });
        let ask = REWRITE_IMAGE_PROMPT.replace("{task}", &task);
        return match text {
            "" => (ask, 4096),
            t => (format!("{ask}{IMAGE_SELECTION_SUFFIX}{t}"), 4096),
        };
    }
    // A prompt that works on the text (the Rewrite button's is REWRITE_PROMPT,
    // copied into config.toml; one without {text} gets it appended, see
    // fill) has nothing to work on: write instead. One built around
    // {instruction} alone still runs. A whole new text (a story, a mail)
    // needs more room than an edit.
    let on_text = own.contains("{text}") || !own.contains("{instruction}");
    if let (true, Some(i)) = (on_text && text.trim().is_empty(), instruction) {
        return (
            keep_spellings(cfg, WRITE_PROMPT.replace("{instruction}", i)),
            4096,
        );
    }
    let mut p = if own.is_empty() { REWRITE_PROMPT } else { own }.to_string();
    if instruction.is_some() && !p.contains("{instruction}") {
        p.push_str("\n\nINSTRUCTION:\n{instruction}");
    }
    let i = instruction.unwrap_or_default();
    let ask = fill(&p, &[("instruction", i), ("text", text)]);
    (keep_spellings(cfg, ask), 1024)
}

fn given(instruction: Option<&str>) -> Option<&str> {
    instruction.map(str::trim).filter(|i| !i.is_empty())
}

/// What [`rewrite_body`] asks, with `{text}` left as the placeholder (the
/// history keeps the text apart): the prompt a replacement shows it came from.
pub fn rewrite_prompt(
    cfg: &Config,
    text: &str,
    own: &str,
    instruction: Option<&str>,
    images: bool,
) -> String {
    let text = if text.trim().is_empty() { "" } else { "{text}" };
    rewrite_ask(cfg, text, own, given(instruction), images).0
}

/// The request of a rewrite: [`rewrite_ask`] with the images.
pub fn rewrite_body(
    cfg: &Config,
    model: &str,
    text: &str,
    own: &str,
    instruction: Option<&str>,
    images: &[String],
) -> Value {
    let instruction = given(instruction);
    let (ask, base) = rewrite_ask(cfg, text, own, instruction, !images.is_empty());
    json!({
        "model": model,
        "messages": [{"role": "user", "content": parts(&ask, images)}],
        // near-greedy: a correction task; an instruction gets room to reformulate
        "temperature": if instruction.is_some() { 0.4 } else { 0.2 },
        "top_p": 0.9,
        "max_tokens": budget(base, text),
        // reasoning measurably HURTS a spec-following edit
        "chat_template_kwargs": {"enable_thinking": false},
    })
}

/// The Settings editor's instruction on `current`, the editor's text. The
/// parts are joined, not filled: both files mention `{text}` in comments.
pub fn settings_body(model: &str, current: &str, instruction: &str) -> Value {
    let user = format!(
        "DEFAULT FILE (everything that can be set):\n{DEFAULT_TOML}\n\n\
         CURRENT FILE:\n{current}\n\nINSTRUCTION:\n{}",
        instruction.trim()
    );
    json!({
        "model": model,
        "messages": [
            {"role": "system", "content": SETTINGS_PROMPT},
            {"role": "user", "content": user},
        ],
        "temperature": 0.2,
        "top_p": 0.9,
        // the answer is the whole file again, plus what gets added
        "max_tokens": budget(512, current),
        "chat_template_kwargs": {"enable_thinking": false},
    })
}

/// OpenAI message content: a string, or parts when images ride along.
fn parts(text: &str, images: &[String]) -> Value {
    if images.is_empty() {
        return text.into();
    }
    let text_part = (!text.is_empty()).then(|| json!({"type": "text", "text": text}));
    let image_parts = images
        .iter()
        .map(|u| json!({"type": "image_url", "image_url": {"url": u}}));
    Value::Array(text_part.into_iter().chain(image_parts).collect())
}

/// Is `text` in the home language? Answered in one word.
pub fn lang_check_body(model: &str, home: &str, text: &str) -> Value {
    let head: String = text.chars().take(2000).collect();
    json!({
        "model": model,
        "messages": [{"role": "user", "content": fill(LANG_DETECT_PROMPT, &[("lang", home), ("text", &head)])}],
        "temperature": 0.0,
        "max_tokens": 8 + REASONING_HEADROOM,
        "chat_template_kwargs": {"enable_thinking": false},
    })
}

/// One chat message as sent: `images` are data URLs.
pub struct Turn<'a> {
    pub role: &'a str,
    pub content: &'a str,
    pub images: &'a [String],
}

/// A multi-turn chat about `selection` (may be empty).
pub fn chat_body(cfg: &Config, model: &str, selection: &str, turns: &[Turn<'_>]) -> Value {
    let system = if selection.is_empty() {
        CHAT_PLAIN_SYSTEM.to_string()
    } else {
        fill(
            prompt(&cfg.prompts.chat, CHAT_SYSTEM_PROMPT),
            &[("text", selection)],
        )
    };
    let mut messages = vec![json!({"role": "system", "content": system})];
    messages.extend(
        turns
            .iter()
            .map(|t| json!({"role": t.role, "content": parts(t.content, t.images)})),
    );
    json!({
        "model": model,
        "messages": messages,
        "temperature": 0.4,
        "top_p": 0.9,
        // a long answer AND a reasoning preamble; also its retries' cap
        "max_tokens": cfg.llm.chat_max_tokens,
        "chat_template_kwargs": {"enable_thinking": cfg.llm.chat_reasoning},
    })
}

/// The think-off retry's system note: the reasoning that ran out of tokens
/// ({notes}). Measured on Sage, 3 looped questions: better answers than a
/// plain retry in 2, a tie in 1, at the same 1–2 s.
const EARLIER_REASONING: &str = "Your earlier reasoning on this question (it did not finish) is below. Use it, then give the final answer directly.\n\n<notes>\n{notes}\n</notes>";

/// Add `notes` ([`EARLIER_REASONING`]) to a body's system message, or as
/// one at its start: strict chat templates take one system message, first.
pub fn add_notes(body: &mut Value, notes: &str) {
    let note = EARLIER_REASONING.replace("{notes}", notes);
    let Some(messages) = body["messages"].as_array_mut() else {
        return;
    };
    match messages.first_mut() {
        Some(m) if m["role"] == "system" && m["content"].is_string() => {
            let system = m["content"].as_str().unwrap_or_default();
            m["content"] = format!("{system}\n\n{note}").into();
        }
        _ => messages.insert(0, json!({"role": "system", "content": note})),
    }
}

/// The translate direction from the language check's answer. When the check
/// failed and home is German, an umlaut/stopword count decides; for another
/// home language the text is taken as foreign.
pub fn is_home(answer: &str, home: &str, text: &str) -> bool {
    let a = answer.trim().trim_matches('.').to_lowercase();
    if a.starts_with("yes") || a.starts_with("ja") {
        return true;
    }
    if a.starts_with("no") || a.starts_with("nein") {
        return false;
    }
    matches!(home.trim().to_lowercase().as_str(), "german" | "deutsch") && looks_german(text)
}

/// Translate's instruction: home → other, anything else → home.
pub fn translate_instruction(from_home: bool, t: &Translate) -> String {
    let (src, dst) = if from_home {
        (t.home.as_str(), t.other.as_str())
    } else {
        ("its original language", t.home.as_str())
    };
    TRANSLATE_PROMPT.replace("{src}", src).replace("{dst}", dst)
}

/// `[llm] hide_models`, case-insensitive.
pub fn hide_regex(pattern: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(pattern).case_insensitive(true).build()
}

/// False for a drafter/embedder/speech model: none of them can chat.
pub fn is_chat_model(hide: &Regex, id: &str) -> bool {
    !hide.is_match(id)
}

#[cfg(test)]
pub(crate) mod tests;
