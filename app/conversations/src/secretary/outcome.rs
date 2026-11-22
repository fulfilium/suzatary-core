//! What a verb's call came to, kept in the clear beside the locked words, so a `remember`'s fact
//! still reads once the owner has erased the answer's words.

use serde::{Deserialize, Serialize};

use suzatary_memory::secretary::memory::FactId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "is", rename_all = "lowercase")]
pub enum Outcome {
    Answered,
    /// The verb refused; the model still sees why.
    Refused,
    /// `remember` wrote fact `fact` to the memory.
    Remembered {
        fact: FactId,
    },
}

impl Outcome {
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
