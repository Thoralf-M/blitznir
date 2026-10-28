//! A chat's title from what was said in it, asked of the picked model when
//! the user wants a better one than the first words of the first message.

use super::{REASONING_HEADROOM, Turn};
use serde_json::{Value, json};

pub const TITLE_PROMPT: &str = "\
Give the conversation below a short title: 3 to 6 words that say what it \
is about, in the language the conversation is in. Output ONLY the title — \
no quotes, no label, no full stop.

CONVERSATION:
{conversation}";

/// The most of a conversation the title is asked about: its start says what
/// it is about, and a long chat would only slow the answer.
const ASKED_CHARS: usize = 6000;
/// A title longer than this is cut; the sidebar shows less anyway.
pub const TITLE_MAX: usize = 60;

/// The request for a title of `turns`.
pub fn title_body(model: &str, turns: &[Turn<'_>]) -> Value {
    let mut said = String::new();
    for t in turns {
        let who = if t.role == "user" {
            "User"
        } else {
            "Assistant"
        };
        said.push_str(&format!("{who}: {}\n\n", t.content.trim()));
        if said.chars().count() > ASKED_CHARS {
            break;
        }
    }
    let said: String = said.chars().take(ASKED_CHARS).collect();
    json!({
        "model": model,
        "messages": [{"role": "user", "content": TITLE_PROMPT.replace("{conversation}", said.trim_end())}],
        "temperature": 0.3,
        "top_p": 0.9,
        "max_tokens": 40 + REASONING_HEADROOM,
        "chat_template_kwargs": {"enable_thinking": false},
    })
}

/// A title as the sidebar keeps it: one line, no quotes or label a model
/// adds anyway, no full stop, at most [`TITLE_MAX`] characters; None when
/// nothing is left.
pub fn clean_title(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = line
        .strip_prefix("Title:")
        .or_else(|| line.strip_prefix("title:"))
        .unwrap_or(line);
    let quotes: &[char] = &['"', '\'', '“', '”', '„', '«', '»', '*', '`'];
    let line = line
        .trim()
        .trim_matches(quotes)
        .trim()
        .trim_end_matches(['.', '!']);
    let title: String = line.chars().take(TITLE_MAX).collect();
    let title = title.trim().to_string();
    (!title.is_empty()).then_some(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_models_answer_becomes_a_title() {
        assert_eq!(
            clean_title("  \"Rust port plan.\"  \n"),
            Some("Rust port plan".into())
        );
        assert_eq!(
            clean_title("Title: **Skia vs webview**\nBecause…"),
            Some("Skia vs webview".into())
        );
        assert_eq!(
            clean_title("„Umzug nach Berlin“"),
            Some("Umzug nach Berlin".into())
        );
        assert_eq!(clean_title(" \n\"\" "), None);
        assert_eq!(
            clean_title(&"x".repeat(90)).unwrap().chars().count(),
            TITLE_MAX
        );
    }

    #[test]
    fn the_request_carries_the_conversation_but_not_all_of_a_long_one() {
        let long = "word ".repeat(3000);
        let turns = [
            Turn {
                role: "user",
                content: "How do I port it?",
                images: &[],
            },
            Turn {
                role: "assistant",
                content: &long,
                images: &[],
            },
            Turn {
                role: "user",
                content: "never asked about",
                images: &[],
            },
        ];
        let b = title_body("m", &turns);
        let asked = b["messages"][0]["content"].as_str().unwrap();
        assert!(asked.contains("User: How do I port it?"));
        assert!(!asked.contains("never asked about"));
        assert!(asked.chars().count() < ASKED_CHARS + TITLE_PROMPT.len());
        assert_eq!(b["chat_template_kwargs"]["enable_thinking"], false);
    }
}
