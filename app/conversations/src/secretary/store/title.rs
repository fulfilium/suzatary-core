//! A conversation's name as a person reads it.

use serde::{Deserialize, Serialize};

/// A conversation's name as a person reads it: one line, never empty, through [`Title::new`]
/// whether a person or the title job gave it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Title(String);

/// Text that is not a title.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotATitle {
    /// Nothing but whitespace.
    #[error("a title needs a word or two")]
    Empty,
    /// A line break or another control character.
    #[error("a title is one line")]
    Lines,
    /// Longer than [`Title::MAX`] characters.
    #[error("a title is at most {} characters", Title::MAX)]
    Long,
}

impl Title {
    /// The longest title, in characters: a list row, not a sentence.
    pub const MAX: usize = 80;

    /// Trim and check one.
    ///
    /// # Errors
    /// [`NotATitle`] naming which rule it broke.
    pub fn new(text: impl Into<String>) -> Result<Self, NotATitle> {
        let text = text.into().trim().to_owned();
        if text.is_empty() {
            Err(NotATitle::Empty)
        } else if text.chars().any(char::is_control) {
            Err(NotATitle::Lines)
        } else if text.chars().count() > Self::MAX {
            Err(NotATitle::Long)
        } else {
            Ok(Self(text))
        }
    }

    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Title {
    type Error = NotATitle;
    fn try_from(text: String) -> Result<Self, NotATitle> {
        Self::new(text)
    }
}

impl std::fmt::Display for Title {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<Title> for String {
    fn from(title: Title) -> String {
        title.0
    }
}
