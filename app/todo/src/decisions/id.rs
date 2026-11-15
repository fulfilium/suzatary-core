use std::fmt;

use super::NotADecision;

/// A decision's name, the key every event about it carries: a todo id's shape, 3-64 lowercase
/// letters and digits starting with a letter, so it prints inside any text unescaped.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecisionId(String);

impl DecisionId {
    /// Check and build one.
    ///
    /// # Errors
    /// `NotADecision::Id` for anything else.
    pub fn new(text: impl Into<String>) -> Result<Self, NotADecision> {
        let text = text.into();
        let shaped = (3..=64).contains(&text.len())
            && text.starts_with(|c: char| c.is_ascii_lowercase())
            && text
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        if shaped {
            Ok(Self(text))
        } else {
            Err(NotADecision::Id(text.chars().take(80).collect()))
        }
    }

    /// The id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DecisionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for DecisionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
