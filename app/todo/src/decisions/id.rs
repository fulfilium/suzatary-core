use std::fmt;

use super::NotADecision;

/// A decision's name, the key every event about it carries: a todo id's shape, 3-64 lowercase
/// letters and digits starting with a letter, so it prints inside any text unescaped.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, prodrome_data::literal::codec::Literal,
)]
#[literal(crate = prodrome_data, new = DecisionId::stored)]
pub struct DecisionId(String);

impl DecisionId {
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

    fn stored(text: String) -> Result<Self, prodrome_data::literal::ProdromeError> {
        Self::new(text)
            .map_err(|failed| prodrome_data::literal::ProdromeError::invalid(failed.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The key a summary of the digest's node `node` is written under: `n` and the node's name
    /// cut to fit an id, so the summary is an event of the decisions' schema with no key of its
    /// own. No decision reads under it.
    #[must_use]
    pub fn of_node(node: &prodrome_data::event::Hash) -> Self {
        Self(
            std::iter::once('n')
                .chain(node.as_str().chars().take(63))
                .collect(),
        )
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
