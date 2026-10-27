//! A conversation's name, and the default every `#/secretary` lands on.

use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// A conversation's name, a slug: one type for the URL segment and the file name, so `..` is
/// refused where the file is opened, as [`crate::chains::ChainName`] does.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ConversationId(String);

/// A name that is not a slug.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "a conversation id is 1-64 characters of a-z, 0-9 and -, starting with a letter or digit, \
     and not `memory`: {0:?}"
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
        // `memory` is taken: `memory.jsonl`, the secretary's memory, shared the directory the
        // conversations' files were kept in.
        let ok = text != "memory"
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

    /// Mint one for today: the day plus the smallest free suffix, so a listing sorts by when.
    ///
    /// The shape it builds always parses, so the `expect` is about this function's own output.
    #[must_use]
    pub fn minted(day: chrono::NaiveDate, taken: &[Self]) -> Self {
        let stem = day.format("%Y-%m-%d").to_string();
        for n in 1u32.. {
            let candidate = Self(format!("{stem}-{n}"));
            if !taken.contains(&candidate) {
                return candidate;
            }
        }
        unreachable!("a u32 of conversations in one day")
    }

    /// Day `day`'s conversation of the box's round `daily`: `<day>-review` or `<day>-sweep`.
    #[must_use]
    pub fn daily(daily: crate::secretary::daily::Daily, day: chrono::NaiveDate) -> Self {
        Self(format!("{}-{}", day.format("%Y-%m-%d"), daily.word()))
    }

    /// Which daily round this is the conversation of, read back from [`ConversationId::daily`]'s
    /// spelling; `None` for any other id.
    #[must_use]
    pub fn daily_round(&self) -> Option<crate::secretary::daily::Daily> {
        use crate::secretary::daily::Daily;
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

/// The default conversation `#/secretary` always lands on, opened lazily by
/// [`crate::secretary::read_or_open_default`]. One fixed slug, not "the newest" (a side topic
/// would silently become tomorrow's default) and not a setting.
///
/// Lazy because [`ConversationId`]'s only constructor validates; the literal is this module's.
pub static DEFAULT: LazyLock<ConversationId> = LazyLock::new(|| {
    ConversationId::new("default").expect("\"default\" is a valid conversation id")
});

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
