//! What a review did about one triage card.

use prodrome_core::literal::ProdromeError;
use serde::{Deserialize, Serialize};

use crate::nonempty::NonEmpty;

/// How a settle ended the objects it named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum Settle {
    /// Adopted onto the chain: a triage batch, before the agent filed cards (2026-10-24).
    Accepted,
    /// Withdrawn by the review: a triage card, or before 2026-10-24 a batch deleted from the
    /// agent's replica.
    Rejected,
}

impl Settle {
    /// The word a record line starts with.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Settle::Accepted => "accepted",
            Settle::Rejected => "rejected",
        }
    }
}

/// What a review did about one batch: settled some of its objects one way, or held it whole.
///
/// Holding is an answer: a batch left standing on purpose is a different fact from one nobody
/// looked at, and the Inbox shows the reason above it. A hold names no object and a settle names
/// at least one, so neither can be written with the other's half.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "decision", rename_all = "lowercase")]
#[ts(rename = "ReviewDecision")]
pub enum Decision {
    /// These objects, accepted or rejected.
    Settled {
        /// Which way.
        how: Settle,
        /// The objects settled.
        #[ts(type = "Array<string>")]
        objects: NonEmpty<String>,
    },
    /// Left standing for the owner, with the reason recorded.
    Held,
}

/// The three answers as words, without the objects: what a list of decisions is grouped by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Accepted or rejected.
    Settled(Settle),
    /// Held.
    Held,
}

impl Outcome {
    /// The three, in the order a list of decisions is printed.
    pub const ALL: [Self; 3] = [
        Self::Settled(Settle::Accepted),
        Self::Settled(Settle::Rejected),
        Self::Held,
    ];
}

impl Decision {
    /// Which of the three answers it is.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self {
            Decision::Settled { how, .. } => Outcome::Settled(*how),
            Decision::Held => Outcome::Held,
        }
    }

    /// The word a record line starts with.
    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            Decision::Settled { how, .. } => how.word(),
            Decision::Held => "held",
        }
    }

    /// The objects it settled: none for a hold.
    #[must_use]
    pub fn objects(&self) -> &[String] {
        match self {
            Decision::Settled { objects, .. } => objects.as_slice(),
            Decision::Held => &[],
        }
    }

    /// A stored line's word and objects read as one decision.
    ///
    /// # Errors
    /// A word that is no answer, a settle naming no object, or a hold naming one.
    pub fn read(word: &str, objects: &[String]) -> Result<Self, ProdromeError> {
        let settled = |how| {
            NonEmpty::of(objects.to_vec())
                .map(|objects| Decision::Settled { how, objects })
                .ok_or_else(|| ProdromeError::invalid(format!("a review {word} no object")))
        };
        match word {
            "accepted" => settled(Settle::Accepted),
            "rejected" => settled(Settle::Rejected),
            "held" if objects.is_empty() => Ok(Decision::Held),
            "held" => Err(ProdromeError::invalid(
                "a hold settles no object, and this one names some",
            )),
            other => Err(crate::literal::no_arm("a review's decision", other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a line stores reads back as the decision written, and only a settle names objects.
    #[test]
    fn a_decision_is_a_settle_of_some_objects_or_a_hold_of_none() {
        let objects = vec!["a1b2".to_owned()];
        let accepted = Decision::read("accepted", &objects).expect("a settle");
        assert_eq!(accepted.objects(), objects.as_slice());
        assert_eq!(accepted.outcome(), Outcome::Settled(Settle::Accepted));
        assert_eq!(Decision::read("held", &[]), Ok(Decision::Held));
        assert!(Decision::read("rejected", &[]).is_err());
        assert!(Decision::read("held", &objects).is_err());
        assert!(Decision::read("maybe", &[]).is_err());
    }
}
