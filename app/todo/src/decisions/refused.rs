/// A decision, or an event about one, that cannot be.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotADecision {
    /// An id not shaped like one.
    #[error("a decision id is 3-64 lowercase letters and digits starting with a letter, got {0:?}")]
    Id(String),
    /// A title that is empty, longer than a line, or broken over lines.
    #[error("a decision's title is one line of 1-120 characters")]
    Title,
    /// A field that must say something and says nothing.
    #[error("a decision's {0} cannot be empty")]
    Empty(&'static str),
    /// A decision naming itself among what it supersedes.
    #[error("{0} cannot supersede itself")]
    Itself(String),
}
