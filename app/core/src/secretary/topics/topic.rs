//! A topic's name, in one spelling: one to [`MAX_WORDS`] words of lowercase letters and digits in
//! any script, joined by single hyphens, at most [`MAX_CHARACTERS`] characters, in NFC. So
//! `Travel`, `travel ` and `travel` cannot be three topics, and a long name cannot be one.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const MAX_WORDS: usize = 3;
pub const MAX_CHARACTERS: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Topic(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotATopic {
    #[error("a topic is written in Unicode's composed form (NFC): {0:?}")]
    NotComposed(String),
    #[error("a topic is lowercase letters and digits, with single hyphens between words: {0:?}")]
    Spelling(String),
    #[error("a topic is at most {MAX_WORDS} words, and {0:?} has {1}")]
    Words(String, usize),
    #[error("a topic is at most {MAX_CHARACTERS} characters, and {0:?} has {1}")]
    Characters(String, usize),
}

impl Topic {
    pub fn new(text: impl Into<String>) -> Result<Self, NotATopic> {
        let text = text.into();
        if !unicode_normalization::is_nfc(&text) {
            return Err(NotATopic::NotComposed(text));
        }
        if !text
            .split('-')
            .all(|word| !word.is_empty() && word.chars().all(letter))
        {
            return Err(NotATopic::Spelling(text));
        }
        let words = text.split('-').count();
        if words > MAX_WORDS {
            return Err(NotATopic::Words(text, words));
        }
        let characters = text.chars().count();
        if characters > MAX_CHARACTERS {
            return Err(NotATopic::Characters(text, characters));
        }
        Ok(Self(text))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `other` is another topic of the same words in any order, each word read with a
    /// plural `-s`, `-es` or `-ies` dropped: `trip-plan` and `plans-trip`, `tax` and `taxes`,
    /// `city` and `cities` are twins.
    #[must_use]
    pub fn twins(&self, other: &Topic) -> bool {
        let one: Vec<&str> = self.0.split('-').collect();
        let two: Vec<&str> = other.0.split('-').collect();
        self != other && one.len() == two.len() && matched(&one, &two)
    }
}

/// Whether each word of `one` is a word of `two` once singular, each word of `two` used once.
fn matched(one: &[&str], two: &[&str]) -> bool {
    let Some((first, rest)) = one.split_first() else {
        return two.is_empty();
    };
    (0..two.len()).any(|at| {
        same_word(first, two[at]) && {
            let mut left = two.to_vec();
            left.remove(at);
            matched(rest, &left)
        }
    })
}

/// Whether two words share a singular: the word itself, or it with a plural ending dropped where
/// at least three letters stay.
fn same_word(one: &str, two: &str) -> bool {
    let singulars = |word: &str| -> BTreeSet<String> {
        let stem = |ending: &str| {
            word.strip_suffix(ending)
                .filter(|stem| stem.chars().count() >= 3)
        };
        [
            Some(word.to_owned()),
            stem("s").map(str::to_owned),
            stem("es").map(str::to_owned),
        ]
        .into_iter()
        .chain([stem("ies").map(|stem| format!("{stem}y"))])
        .flatten()
        .collect()
    };
    !singulars(one).is_disjoint(&singulars(two))
}

/// A letter or a digit that is its own lowercase: `A` is not, `a`, `ß`, `٣` and `ع` are.
fn letter(c: char) -> bool {
    c.is_alphanumeric() && c.to_lowercase().eq(std::iter::once(c))
}

impl TryFrom<String> for Topic {
    type Error = NotATopic;

    fn try_from(text: String) -> Result<Self, NotATopic> {
        Self::new(text)
    }
}

impl From<Topic> for String {
    fn from(topic: Topic) -> Self {
        topic.0
    }
}

impl std::fmt::Display for Topic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

prodrome_core::text_literal!(
    Topic => |topic| topic.as_str().to_owned(),
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_topic_has_one_spelling_and_is_short() {
        for good in [
            "travel",
            "trip-abroad",
            "taxes-2026-filing",
            "2026",
            "straße",
            "سفر",
            "日本",
        ] {
            assert_eq!(Topic::new(good).map(String::from), Ok(good.to_owned()));
        }
        for bad in [
            "", "Travel", "travel ", " travel", "-travel", "travel-", "trip--to", "trip to", "a/b",
            "a_b", "İ",
        ] {
            assert!(
                matches!(Topic::new(bad), Err(NotATopic::Spelling(_))),
                "{bad}"
            );
        }
        assert_eq!(
            Topic::new("a-b-c-d"),
            Err(NotATopic::Words("a-b-c-d".to_owned(), 4))
        );
        let long = "a".repeat(MAX_CHARACTERS + 1);
        assert_eq!(
            Topic::new(long.clone()),
            Err(NotATopic::Characters(long, MAX_CHARACTERS + 1))
        );
        assert!(Topic::new("a".repeat(MAX_CHARACTERS)).is_ok());
        // `e` and a combining acute accent read as `é`, which NFC writes as one character.
        assert!(matches!(
            Topic::new("cafe\u{301}"),
            Err(NotATopic::NotComposed(_))
        ));
    }

    #[test]
    fn a_topic_twins_the_same_words_in_any_order_and_number() {
        let topic = |text: &str| Topic::new(text).expect("a topic");
        for (one, two) in [
            ("trip-plan", "plans-trip"),
            ("tax", "taxes"),
            ("city", "cities"),
            ("note", "notes"),
            ("garden", "gardens"),
        ] {
            assert!(topic(one).twins(&topic(two)), "{one} {two}");
            assert!(topic(two).twins(&topic(one)), "{two} {one}");
        }
        for (one, two) in [
            ("garden", "garden"),
            ("trip", "trip-plan"),
            ("bus", "bu"),
            ("tax-return", "tax-returns-due"),
            ("plan-plan", "plan-trip"),
        ] {
            assert!(!topic(one).twins(&topic(two)), "{one} {two}");
        }
    }
}
