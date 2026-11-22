//! A review that could not run.

use suzatary_memory::secretary::words::Words;

/// A review that could not run, which must not read as a considered hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewFailure {
    pub at: suzatary_base::capability::Datetime,
    pub reason: Words<String>,
    /// How many cards stood unanswered at that moment.
    pub standing: usize,
}

impl std::fmt::Display for ReviewFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "could not review {} card(s): {}",
            self.standing, self.reason
        )
    }
}
