//! What the panel may run now, and how a translation lands.

use serde::Deserialize;

/// What Return or Translate in the panel asks for. Return is one rewrite:
/// a clean-up without an instruction, by the instruction with one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Rewrite,
    Translate,
    /// The Settings editor's instruction: never pasted, never copied.
    Settings,
}

pub const STILL_COPYING: &str = "Still copying the selection: press Return again in a moment";

/// Why the panel cannot run `kind` now, or None. `selection` is None while
/// the ⌘C is still out; images are input as much as text is.
pub fn cannot_run(
    kind: Kind,
    selection: Option<&str>,
    instruction: &str,
    images: usize,
) -> Option<&'static str> {
    if kind == Kind::Settings {
        return instruction
            .trim()
            .is_empty()
            .then_some("Type what to change first");
    }
    let Some(text) = selection else {
        return Some(STILL_COPYING);
    };
    let nothing = text.trim().is_empty() && images == 0;
    match kind {
        Kind::Translate if nothing => Some("Nothing to translate: select text or type it below"),
        Kind::Rewrite if nothing && instruction.trim().is_empty() => {
            Some("Nothing selected: type what you want, paste an image, or chat")
        }
        _ => None,
    }
}

/// Translate with nothing selected and no image: the typed text is what is
/// translated, in the panel's field (not an instruction, as for Rewrite).
pub fn translates_typed(kind: Kind, selection: Option<&str>, typed: &str, images: usize) -> bool {
    kind == Kind::Translate
        && selection.is_some_and(|s| s.trim().is_empty())
        && images == 0
        && !typed.trim().is_empty()
}

/// A translation replaces the selection unless the app says the
/// focused element takes no text (a web page, a mail being read).
pub fn translate_pastes(editable: Option<bool>) -> bool {
    editable != Some(false)
}

/// The user turn that stands for a translation shown in the chat.
/// Whether Retry may ask `question` again as a chat turn: not a
/// translation's ([`translation_turn`]), whose answer came from Translate's
/// own prompt, not the chat's.
pub fn retryable(question: &str) -> bool {
    question != "Translate" && !question.starts_with("Translate\n\n")
}

pub fn translation_turn(text: &str) -> String {
    match text.trim() {
        "" => "Translate".into(),
        t => format!("Translate\n\n{t}"),
    }
}

/// A chat message that is only images asks this.
pub const IMAGE_QUESTION: &str = "What do you see in this image?";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;
    use crate::platform::fake::FakePlatform;

    #[test]
    fn translate_replaces_unless_the_app_says_read_only() {
        // as the panel asks: the platform's answer for the app it came from
        let mut p = FakePlatform::new();
        for (editable, pastes) in [(Some(true), true), (None, true), (Some(false), false)] {
            p.editable = editable;
            let asked = p.focused_editable(&p.focused_app());
            assert_eq!(translate_pastes(asked), pastes, "{editable:?}");
        }
    }

    #[test]
    fn translate_with_nothing_selected_takes_the_typed_text() {
        assert!(translates_typed(Kind::Translate, Some(""), "Ein Nest", 0));
        assert!(
            !translates_typed(Kind::Translate, Some("a selection"), "Ein Nest", 0),
            "the selection"
        );
        assert!(
            !translates_typed(Kind::Translate, None, "Ein Nest", 0),
            "still copying"
        );
        assert!(
            !translates_typed(Kind::Translate, Some(""), "Ein Nest", 1),
            "an image"
        );
        assert!(
            !translates_typed(Kind::Translate, Some(""), "  ", 0),
            "nothing typed"
        );
        assert!(
            !translates_typed(Kind::Rewrite, Some(""), "Ein Nest", 0),
            "an instruction"
        );
    }

    #[test]
    fn the_panel_says_why_it_cannot_run() {
        // as the panel reads the selection: no copy = nothing selected
        let p = FakePlatform::new();
        let copied = || p.finish_copy(p.begin_copy()).unwrap_or_default();
        assert!(cannot_run(Kind::Translate, Some(&copied()), "", 0).is_some());
        *p.selection.lock().unwrap() = Some("äh hallo".into());
        assert!(cannot_run(Kind::Rewrite, Some(&copied()), "", 0).is_none());
        assert!(cannot_run(Kind::Translate, Some(&copied()), "", 0).is_none());
        assert_eq!(cannot_run(Kind::Rewrite, None, "", 0), Some(STILL_COPYING));
        assert_eq!(
            cannot_run(Kind::Translate, None, "", 1),
            Some(STILL_COPYING)
        );
        assert!(cannot_run(Kind::Rewrite, Some("  "), "", 0).is_some());
        assert!(
            cannot_run(Kind::Translate, Some(""), "", 1).is_none(),
            "an image is input"
        );
        assert!(
            cannot_run(Kind::Rewrite, Some(""), "write a haiku", 0).is_none(),
            "an instruction without a selection generates"
        );
        assert!(cannot_run(Kind::Rewrite, Some(""), " ", 0).is_some());
        // Settings works on the editor's text: the selection plays no part
        assert!(cannot_run(Kind::Settings, None, " ", 0).is_some());
        assert!(cannot_run(Kind::Settings, None, "set double_tap_ms to 600", 0).is_none());
    }

    #[test]
    fn a_shown_translation_is_a_chat_turn() {
        assert_eq!(translation_turn(" Hallo \n"), "Translate\n\nHallo");
        assert_eq!(translation_turn(""), "Translate");
        assert!(!retryable(&translation_turn("Hallo")) && !retryable(&translation_turn("")));
        assert!(retryable("Translate this for me"));
        let k: Kind = serde_json::from_str("\"translate\"").unwrap();
        assert_eq!(k, Kind::Translate);
        let k: Kind = serde_json::from_str("\"rewrite\"").unwrap();
        assert_eq!(k, Kind::Rewrite);
    }
}
