//! A summary's words: one line of at most `NODE_BYTES`, through the redaction door.
//!
//! [`Line::new`] is the only door, for words read back from the text store as for words a model
//! wrote, so a summary can hold no line break, no identity number and no link's credential: what
//! a conversation line could not be stored with, its summary cannot either.

use crate::redact::Scrubbed;
use crate::secretary::salted_text::Words;

/// The most bytes a summary may hold: a node of the digest costs at most this in a view.
pub(crate) const NODE_BYTES: usize = 512;

/// One summary line, scrubbed, trimmed, nonempty, at most `NODE_BYTES`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line(String);

/// Words that are no summary line.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotALine {
    /// Nothing but space.
    #[error("a summary says something")]
    Empty,
    /// A line break or another control character.
    #[error("a summary is one line")]
    LineBreak,
    /// Longer than `NODE_BYTES` once scrubbed.
    #[error("a summary is at most {NODE_BYTES} bytes, got {0}")]
    TooLong(usize),
    /// The redaction door refused it.
    #[error("{0}")]
    Unscrubbable(String),
}

impl Line {
    /// The words, trimmed and scrubbed.
    ///
    /// # Errors
    /// [`NotALine`] naming the rule the words break.
    pub fn new(words: &str) -> Result<Self, NotALine> {
        let scrubbed = Scrubbed::of(&words.trim().to_owned())
            .map_err(|refused| NotALine::Unscrubbable(refused.to_string()))?
            .into_inner();
        if scrubbed.is_empty() {
            return Err(NotALine::Empty);
        }
        if scrubbed
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        {
            return Err(NotALine::LineBreak);
        }
        if scrubbed.len() > NODE_BYTES {
            return Err(NotALine::TooLong(scrubbed.len()));
        }
        Ok(Self(scrubbed))
    }

    /// The words.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Kept as written: the door it was made through is the door it is read back through.
impl Words for Line {
    fn print(&self) -> std::borrow::Cow<'_, str> {
        self.0.as_str().into()
    }
    fn read(print: &str) -> Option<Self> {
        Self::new(print).ok().filter(|line| line.0 == print)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        /// The store's law, and the door is idempotent: a line made again is itself.
        #[test]
        fn a_line_reads_back_as_itself(words in "(?s).{0,600}") {
            if let Ok(line) = Line::new(&words) {
                prop_assert_eq!(Line::read(&line.print()), Some(line.clone()));
                prop_assert_eq!(Line::new(line.as_str()), Ok(line.clone()));
                prop_assert!(line.as_str().len() <= NODE_BYTES);
            }
        }
    }

    #[test]
    fn a_line_is_one_short_scrubbed_line() {
        assert_eq!(Line::new("  "), Err(NotALine::Empty));
        assert_eq!(Line::new("one\ntwo"), Err(NotALine::LineBreak));
        assert_eq!(
            Line::new(&"a".repeat(NODE_BYTES + 1)),
            Err(NotALine::TooLong(NODE_BYTES + 1))
        );
        assert!(Line::new(&"a".repeat(NODE_BYTES)).is_ok());
        let scrubbed = Line::new("Her SSN is 123-45-6789, said on the call").expect("a line");
        assert!(!scrubbed.as_str().contains("6789"), "{}", scrubbed.as_str());
    }
}
