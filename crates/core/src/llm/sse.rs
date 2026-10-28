//! The SSE lines of a streamed chat completion, and whether what they add
//! up to is a whole answer. Only a whole one may reach the user's selection:
//! a cut-off rewrite pasted over it replaces text the clipboard no longer has.

use crate::{Error, Result};
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

/// One SSE line.
#[derive(Debug, PartialEq)]
pub(super) enum Line {
    Skip,
    Done,
    Delta {
        content: String,
        reasoning: usize,
        /// Set on the last chunk: "stop", "length", …
        finish: Option<String>,
    },
    /// An error the server sent inside the stream, after its 200.
    Failed(String),
}

pub(super) fn parse_line(raw: &str) -> Line {
    let Some(data) = raw.trim().strip_prefix("data:") else {
        return Line::Skip;
    };
    let data = data.trim();
    if data == "[DONE]" {
        return Line::Done;
    }
    let Ok(v) = serde_json::from_str::<Value>(data) else {
        return Line::Skip;
    };
    if !v["error"].is_null() {
        return Line::Failed(error_message(&v).unwrap_or_else(|| v["error"].to_string()));
    }
    let choice = &v["choices"][0];
    let text = |k: &str| choice["delta"][k].as_str().unwrap_or_default().to_string();
    // llama.cpp / LM Studio say reasoning_content, oMLX says reasoning
    let reasoning = [text("reasoning_content"), text("reasoning")]
        .into_iter()
        .find(|s| !s.is_empty());
    Line::Delta {
        content: text("content"),
        reasoning: reasoning.map_or(0, |r| r.chars().count()),
        finish: choice["finish_reason"].as_str().map(str::to_owned),
    }
}

/// `{"error": {"message": …}}` or `{"error": "…"}`: the server's own words.
pub(super) fn error_message(v: &Value) -> Option<String> {
    let e = &v["error"];
    e["message"]
        .as_str()
        .or_else(|| e.as_str())
        .map(str::to_owned)
}

/// The fence's whole line, whatever language it names (c++, c#,
/// objective-c); in a fence of one line, its letters ("```json{…}```").
static FENCE_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^```(?:[^\n`]*\n|[a-zA-Z]*)").expect("constant regex"));
static FENCE_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\n?```$").expect("constant regex"));

/// Strip the wrappers a chat model likes to put around a bare answer: a code
/// fence, then one pair of the SAME quote character.
pub(super) fn strip_wrappers(text: &str) -> String {
    let mut text = text.trim().to_string();
    if text.starts_with("```") {
        let open = FENCE_OPEN.replace(&text, "").into_owned();
        text = FENCE_CLOSE.replace(&open, "").trim().to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    if let [first, .., last] = chars[..]
        && first == last
        && "\"'“”".contains(first)
    {
        return chars[1..chars.len() - 1]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
    }
    text
}

/// How one streamed attempt ended.
#[derive(Debug, PartialEq)]
pub(super) enum Ended {
    /// "stop", or no finish_reason and a proper [DONE].
    Whole,
    /// "length": the token budget ran out mid-answer.
    Budget,
    /// An error in the stream, another finish_reason, or no [DONE].
    Broken(String),
}

/// The stream's last finish_reason, and whether [DONE] came.
pub(super) fn ended(finish: Option<&str>, done: bool) -> Ended {
    match finish {
        Some("stop") => Ended::Whole,
        None if done => Ended::Whole,
        Some("length") => Ended::Budget,
        None => Ended::Broken("the stream ended before the answer did".into()),
        Some(other) => Ended::Broken(format!("the model stopped early ({other})")),
    }
}

/// One attempt's result.
pub(super) struct Attempt {
    /// Unwrapped, as its task wants it.
    pub content: String,
    /// Characters of reasoning seen.
    pub reasoning: usize,
    pub ended: Ended,
}

impl Attempt {
    /// Out of tokens: cut off, or reasoned without answering. Worth one
    /// retry with a bigger budget.
    pub fn starved(&self) -> bool {
        self.ended == Ended::Budget
            || (self.ended == Ended::Whole && self.content.is_empty() && self.reasoning > 0)
    }

    /// The answer, only when it is whole and not empty; `budget` was its
    /// max_tokens. `asked`: the request asked for reasoning (`[llm]
    /// chat_reasoning`), so the advice is to turn that off, not to change
    /// a model that only did as asked.
    pub fn answer(self, budget: usize, asked: bool) -> Result<String> {
        let n = self.reasoning;
        let fix = if asked {
            "it thought too long: turn off Think, next to the model ([llm] chat_reasoning)"
        } else {
            "pick a model that does not reason"
        };
        match self.ended {
            Ended::Broken(why) => Err(Error::Llm(why)),
            Ended::Budget if n > 0 => Err(Error::Llm(format!(
                "the answer was cut off at max_tokens={budget} after {n} chars of reasoning; {fix}"
            ))),
            Ended::Budget => Err(Error::Llm(format!(
                "the answer was cut off at max_tokens={budget}"
            ))),
            Ended::Whole if !self.content.is_empty() => Ok(self.content),
            Ended::Whole if n == 0 => Err(Error::Llm("the model returned nothing".into())),
            Ended::Whole => Err(Error::Llm(format!(
                "the model returned nothing — it spent the whole token budget reasoning ({n} chars); {fix}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_starved_answer_says_what_to_change() {
        let cut = |asked| {
            let a = Attempt {
                content: "half".into(),
                reasoning: 900,
                ended: Ended::Budget,
            };
            a.answer(8192, asked).unwrap_err().to_string()
        };
        // asked to reason: the setting, not the model, is the thing to change
        assert!(cut(true).contains("turn off Think"), "{}", cut(true));
        assert!(cut(false).contains("pick a model that does not reason"));
    }

    #[test]
    fn sse_lines_parse() {
        assert_eq!(parse_line("event: x"), Line::Skip);
        assert_eq!(parse_line("data:[DONE]\r"), Line::Done);
        let both = r#"data: {"choices":[{"delta":{"content":"a","reasoning_content":"xyz"}}]}"#;
        let delta = |content: &str, reasoning, finish: Option<&str>| Line::Delta {
            content: content.into(),
            reasoning,
            finish: finish.map(str::to_owned),
        };
        assert_eq!(parse_line(both), delta("a", 3, None));
        assert_eq!(parse_line(r#"data: {"usage":{}}"#), delta("", 0, None));
        let last = r#"data: {"choices":[{"delta":{},"finish_reason":"length"}]}"#;
        assert_eq!(parse_line(last), delta("", 0, Some("length")));
        let failed = r#"data: {"error":{"message":"out of memory","code":500}}"#;
        assert_eq!(parse_line(failed), Line::Failed("out of memory".into()));
        assert_eq!(
            parse_line(r#"data: {"error":{"code":500}}"#),
            Line::Failed(r#"{"code":500}"#.into())
        );
    }

    #[test]
    fn only_stop_or_a_plain_done_is_whole() {
        assert_eq!(ended(Some("stop"), true), Ended::Whole);
        assert_eq!(ended(Some("stop"), false), Ended::Whole);
        assert_eq!(ended(None, true), Ended::Whole);
        assert_eq!(ended(Some("length"), true), Ended::Budget);
        assert!(matches!(ended(None, false), Ended::Broken(_)));
        assert!(
            matches!(ended(Some("content_filter"), true), Ended::Broken(w) if w.contains("content_filter"))
        );
    }
}
