//! A review that could not run.

use crate::secretary::words::Words;

/// A review that could not run, as a reader gets it: a sibling of [`Review`](super::Review) rather than a fourth
/// [`Decision`](super::Decision), since it is about every card at once and must not read as a considered hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewFailure {
    /// When it was attempted.
    pub at: suzatary_base::capability::Datetime,
    /// The box's own words for why.
    pub reason: Words<String>,
    /// How many cards stood unanswered at that moment.
    pub standing: usize,
}

impl std::fmt::Display for ReviewFailure {
    /// One line, shaped like [`Review`](super::Review)'s so a day's log is one column of sentences.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "could not review {} card(s): {}",
            self.standing, self.reason
        )
    }
}
