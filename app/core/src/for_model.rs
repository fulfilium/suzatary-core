//! Text on its way to a model. Every model client takes [`ForModel`] alone, and each of its
//! constructors masks, so no path sends a model text the masker did not read.

use crate::held_value::withheld;

/// Text a model may read: every held number in it a placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForModel(String);

impl ForModel {
    /// `text`, masked.
    #[must_use]
    pub fn of(text: &str) -> Self {
        Self(withheld(text))
    }

    /// A file's name, masked as a placeholder shows it.
    #[must_use]
    pub fn file_name(name: &str) -> Self {
        Self(crate::held_value::file_name(name))
    }

    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The text, owned.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::ops::Deref for ForModel {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ForModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
