//! What a chat message may carry beside its text: why an answer came
//! without its thinking, and why a turn got no answer.

use serde::{Deserialize, Serialize};

/// Why an answer came without the thinking it was asked for: its reasoning
/// ran past the token ceiling, or looped; the trace kept is that one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unthought {
    #[default]
    No,
    RanOut,
    Looped,
}

impl Unthought {
    pub fn is_no(&self) -> bool {
        *self == Unthought::No
    }
}

/// A user's turn that got no answer: it stays in its chat, marked, to be
/// sent again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unanswered {
    Cancelled,
    /// The error, in the words the status line used.
    Failed(String),
}

impl Unanswered {
    /// As shown under the turn.
    pub fn says(&self) -> String {
        match self {
            Unanswered::Cancelled => "Cancelled".into(),
            Unanswered::Failed(why) => format!("Failed: {why}"),
        }
    }
}
