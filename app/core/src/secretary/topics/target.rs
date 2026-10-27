//! What a topic is put on, named by a turn index, which never moves because only the box appends
//! a conversation's turns. The one spelling is `round:<conversation>/<turn>` or
//! `note:<conversation>/<turn>`, with no leading zero.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::secretary::store::ConversationId;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Target {
    /// The round that turn `turn` opened.
    Round {
        conversation: ConversationId,
        turn: usize,
    },
    /// The turn `draft_note` wrote.
    Note {
        conversation: ConversationId,
        turn: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a topic's target is round:<conversation>/<turn> or note:<conversation>/<turn>, got {0:?}")]
pub struct NotATarget(pub String);

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (scheme, conversation, turn) = match self {
            Self::Round { conversation, turn } => ("round", conversation, turn),
            Self::Note { conversation, turn } => ("note", conversation, turn),
        };
        write!(f, "{scheme}:{}/{turn}", conversation.as_str())
    }
}

impl FromStr for Target {
    type Err = NotATarget;

    fn from_str(text: &str) -> Result<Self, NotATarget> {
        let refused = || NotATarget(text.to_owned());
        let (scheme, rest) = text.split_once(':').ok_or_else(refused)?;
        let (conversation, turn) = rest.rsplit_once('/').ok_or_else(refused)?;
        let conversation = ConversationId::new(conversation).map_err(|_| refused())?;
        let turn: usize = Some(turn)
            .filter(|turn| {
                turn.bytes().all(|b| b.is_ascii_digit()) && (*turn == "0" || !turn.starts_with('0'))
            })
            .and_then(|turn| turn.parse().ok())
            .ok_or_else(refused)?;
        match scheme {
            "round" => Ok(Self::Round { conversation, turn }),
            "note" => Ok(Self::Note { conversation, turn }),
            _ => Err(refused()),
        }
    }
}

impl TryFrom<String> for Target {
    type Error = NotATarget;

    fn try_from(text: String) -> Result<Self, NotATarget> {
        text.parse()
    }
}

impl From<Target> for String {
    fn from(target: Target) -> Self {
        target.to_string()
    }
}

prodrome_core::text_literal!(
    Target => ToString::to_string,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_has_one_spelling() {
        for good in ["round:default/0", "round:2026-10-21-1/12", "note:default/7"] {
            assert_eq!(
                good.parse::<Target>().map(|t| t.to_string()),
                Ok(good.to_owned())
            );
        }
        for bad in [
            "round:default/012",
            "round:default/",
            "round:default/-1",
            "round:default/+1",
            "round:/1",
            "round:Default/1",
            "round:../1",
            "turn:default/1",
            "default/1",
            "round:default/1/2",
            "round:default/99999999999999999999999",
        ] {
            assert!(bad.parse::<Target>().is_err(), "{bad}");
        }
    }
}
