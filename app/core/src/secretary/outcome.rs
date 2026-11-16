//! What a verb's call came to, kept with the turn's shape in the clear: answered, refused, or
//! a fact remembered.
//!
//! The fact a `remember` call wrote is its receipt. It lives here, beside the locked words and not
//! in them, so it still reads once the owner has erased the answer's words.

use serde::{Deserialize, Serialize};

use suzatary_memory::secretary::memory::FactId;

/// How one call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "is", rename_all = "lowercase")]
pub enum Outcome {
    /// The verb answered.
    Answered,
    /// The verb refused; the model still sees why.
    Refused,
    /// `remember` wrote fact `fact` to the memory.
    Remembered {
        /// The fact it wrote.
        fact: FactId,
    },
}

impl Outcome {
    /// Whether the call was refused.
    #[must_use]
    pub fn refused(self) -> bool {
        match self {
            Outcome::Refused => true,
            Outcome::Answered | Outcome::Remembered { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each outcome has one print, and reads back as itself.
    #[test]
    fn each_outcome_prints_once_and_reads_back() {
        for (outcome, print) in [
            (Outcome::Answered, r#"{"is":"answered"}"#),
            (Outcome::Refused, r#"{"is":"refused"}"#),
            (
                Outcome::Remembered { fact: FactId(7) },
                r#"{"is":"remembered","fact":7}"#,
            ),
        ] {
            assert_eq!(serde_json::to_string(&outcome).expect("prints"), print);
            assert_eq!(
                serde_json::from_str::<Outcome>(print).expect("reads"),
                outcome
            );
        }
        assert!(
            serde_json::from_str::<Outcome>(r#"{"is":"remembered"}"#).is_err(),
            "a remembered outcome names its fact"
        );
    }
}
