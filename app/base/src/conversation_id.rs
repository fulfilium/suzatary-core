//! A conversation's name, and the box's daily rounds that name one a day.

use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// A conversation's name, a slug: one type for the URL segment and the file name, so `..` is
/// refused where the file is opened, as [`crate::chains::ChainName`] does.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ConversationId(String);

/// Text that names no single held conversation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unmatched {
    /// No held id is or begins with it.
    #[error("no conversation is or begins with {0}")]
    None(String),
    /// Several held ids begin with it.
    #[error("{count} conversations begin with {text}: type more of the id")]
    Several {
        /// The text.
        text: String,
        /// How many begin with it.
        count: usize,
    },
}

/// A name that is not a slug.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "a conversation id is 1-64 characters of a-z, 0-9 and -, starting with a letter or digit, \
     and not `memory` or `new`: {0:?}"
)]
pub struct NotAnId(pub String);

impl ConversationId {
    /// Parse a slug.
    ///
    /// # Errors
    /// When it is empty, too long, starts with a hyphen, or holds anything but
    /// lowercase letters, digits and hyphens.
    pub fn new(text: impl Into<String>) -> Result<Self, NotAnId> {
        let text = text.into();
        // `new` is the page's address of a topic not yet opened (`#/secretary/new`).
        let ok = text != "memory"
            && text != "new"
            && matches!(text.len(), 1..=64)
            && text.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && text
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if ok {
            Ok(Self(text))
        } else {
            Err(NotAnId(text))
        }
    }

    /// The id among `held` that `text` names: that id itself, or the one held id `text` begins,
    /// so a person types a few of a conversation's 32 hex digits rather than all of them.
    ///
    /// # Errors
    /// [`Unmatched`]: no held id is or begins with `text`, or several begin with it.
    pub fn among(text: &str, held: &[Self]) -> Result<Self, Unmatched> {
        if let Some(id) = held.iter().find(|id| id.0 == text) {
            return Ok(id.clone());
        }
        let begun: Vec<&Self> = held.iter().filter(|id| id.0.starts_with(text)).collect();
        match begun.as_slice() {
            [one] => Ok((*one).clone()),
            [] => Err(Unmatched::None(text.to_owned())),
            several => Err(Unmatched::Several {
                text: text.to_owned(),
                count: several.len(),
            }),
        }
    }

    /// Day `day`'s conversation of the box's round `daily`: `<day>-review` or `<day>-sweep`.
    #[must_use]
    pub fn daily(daily: Daily, day: chrono::NaiveDate) -> Self {
        Self(format!("{}-{}", day.format("%Y-%m-%d"), daily.word()))
    }

    /// Which daily round this is the conversation of, read back from [`ConversationId::daily`]'s
    /// spelling; `None` for any other id.
    #[must_use]
    pub fn daily_round(&self) -> Option<Daily> {
        let (day, word) = self.0.split_at_checked(10)?;
        chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
        [Daily::Review, Daily::Sweep]
            .into_iter()
            .find(|daily| word.strip_prefix('-') == Some(daily.word()))
    }

    /// The name, as a path segment and a file stem spell it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A round the box opens for itself, once a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Daily {
    /// The review of the triage agent's open cards.
    Review,
    /// The sweep of the open list.
    Sweep,
}

impl Daily {
    /// What its conversation's id ends with, after the day.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Daily::Review => "review",
            Daily::Sweep => "sweep",
        }
    }

    /// What its conversation is called in a list.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Daily::Review => "Review",
            Daily::Sweep => "Sweep",
        }
    }
}

/// The default conversation `#/secretary` lands on, opened lazily by
/// `suzatary_core::secretary::conversations::read_or_open_default`.
pub static DEFAULT: LazyLock<ConversationId> = LazyLock::new(|| {
    ConversationId::new("default").expect("\"default\" is a valid conversation id")
});

/// The box's own conversation, where it files the cards it raises itself.
pub static SETUP: LazyLock<ConversationId> =
    LazyLock::new(|| ConversationId::new("setup").expect("\"setup\" is a conversation id"));

impl TryFrom<String> for ConversationId {
    type Error = NotAnId;
    fn try_from(text: String) -> Result<Self, NotAnId> {
        Self::new(text)
    }
}

impl From<ConversationId> for String {
    fn from(id: ConversationId) -> String {
        id.0
    }
}

impl std::fmt::Display for ConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ConversationId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

// Its literal is its text, as a stored line prints it.
prodrome_core::text_literal!(
    ConversationId => |id| id.as_str().to_owned(),
);

#[cfg(test)]
mod tests {
    use super::*;

    /// A name is a path segment and a file stem.
    #[test]
    fn a_conversation_id_is_a_slug() {
        for good in ["2026-09-09-1", "brief", "a", "9", &"a".repeat(64)] {
            assert!(ConversationId::new(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "..",
            "-lead",
            "Brief",
            "has space",
            "has/slash",
            "has_underscore",
            "%2e%2e",
            "memory",
            "new",
            &"a".repeat(65),
        ] {
            assert!(ConversationId::new(bad).is_err(), "{bad:?}");
        }
    }

    /// An id is parsed on the way in, wherever it comes from, including a stored line somebody
    /// could have edited.
    #[test]
    fn an_id_is_parsed_when_it_is_deserialised() {
        assert!(serde_json::from_str::<ConversationId>(r#""../etc""#).is_err());
        assert_eq!(
            serde_json::from_str::<ConversationId>(r#""brief-1""#)
                .expect("a slug")
                .as_str(),
            "brief-1"
        );
    }

    /// A held id is named by itself or by a prefix only it begins with; a prefix two begin with,
    /// or none, names nothing.
    #[test]
    fn a_conversation_is_named_by_a_prefix_only_it_begins_with() {
        let held: Vec<ConversationId> = ["0a1b2c", "0a9f00", "default"]
            .into_iter()
            .map(|id| ConversationId::new(id).expect("a slug"))
            .collect();
        let named = |text: &str| ConversationId::among(text, &held).map(|id| id.to_string());
        assert_eq!(named("default"), Ok("default".to_owned()));
        assert_eq!(named("0a1"), Ok("0a1b2c".to_owned()));
        assert_eq!(named("0a9f00"), Ok("0a9f00".to_owned()));
        assert_eq!(
            named("0a"),
            Err(Unmatched::Several {
                text: "0a".to_owned(),
                count: 2
            })
        );
        assert_eq!(named("ff"), Err(Unmatched::None("ff".to_owned())));
    }
}
