//! One review answer about a triage card, or a batch before cards, as a reader gets it.

use super::Decision;
use crate::secretary::words::Words;

/// One review answer, as a reader gets it: the fold of one [`Entry::Reviewed`](super::Entry::Reviewed). A review is a
/// fact about a moment, so re-reviewing a head later is another line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    /// When it was decided.
    pub at: suzatary_base::capability::Datetime,
    /// Which chain the card was against.
    pub chain: String,
    /// What was reviewed: a card's reference, or an older batch's head.
    pub batch: String,
    /// What was decided, and the objects it settled.
    pub decision: Decision,
    /// Why.
    pub why: Words<String>,
}

impl std::fmt::Display for Review {
    /// The record line, one shape for every decision, so every reader quotes one sentence:
    /// `rejected proposal:triage/3 on life: <why>` and `held proposal:triage/4 on life: <why>`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} on {}: {}",
            self.decision.word(),
            self.batch,
            self.chain,
            self.why
        )
    }
}
