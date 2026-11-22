//! One review answer about a triage card, or a batch before cards, as a reader gets it.

use super::Decision;
use suzatary_memory::secretary::words::Words;

/// One review answer: a fact about a moment, so re-reviewing a head later is another line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    pub at: suzatary_base::capability::Datetime,
    pub chain: String,
    /// What was reviewed: a card's reference, or an older batch's head.
    pub batch: String,
    pub decision: Decision,
    pub why: Words<String>,
}

impl std::fmt::Display for Review {
    /// The record line, one shape for every decision:
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
