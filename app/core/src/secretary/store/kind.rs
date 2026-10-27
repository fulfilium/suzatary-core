//! What a conversation is for.

use serde::{Deserialize, Serialize};

/// What a conversation is for: the list pins the default, sorts topics by their last turn, and
/// files the box's own rounds behind a filter.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "ConversationKind")]
pub enum Kind {
    /// [`DEFAULT`](super::DEFAULT), the one `#/secretary` always has.
    Default,
    /// One the owner opened, about one thing.
    Topic,
    /// One the box opened for itself: `secretary review`'s and the sweep's.
    Review,
}

impl Kind {
    /// Every kind.
    pub const ALL: [Kind; 3] = [Kind::Default, Kind::Topic, Kind::Review];

    /// The name a stored line prints.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Default => "default",
            Kind::Topic => "topic",
            Kind::Review => "review",
        }
    }
}

// Its literal is its text, as a stored line prints it.
prodrome_core::word_literal!(Kind: Kind::ALL => |kind| kind.as_str());

#[cfg(test)]
mod tests {
    use super::*;

    /// `rename_all` spells the generated `ConversationKind`, so serde's words are checked against `ALL`'s.
    #[test]
    fn a_kinds_serde_word_is_its_literals() {
        crate::words::assert_serde_words(&Kind::ALL, |kind| kind.as_str());
    }
}
