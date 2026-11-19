//! One half of one tree, as the table's rows say.

use crate::{Outcome, Row};
use std::fmt;

/// The union of the outcomes the rows hold: a diamond with `Unjudged` (no row) at the bottom,
/// `Finding` (green and red both seen, so the gate read something beyond the tree) at the top, and
/// the two agreements between them, neither above the other.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reading {
    Unjudged,
    Agreed(Outcome),
    Finding,
}

impl Reading {
    /// The least upper bound in the diamond.
    #[must_use]
    pub fn join(self, other: Reading) -> Reading {
        match (self, other) {
            (Reading::Unjudged, reading) | (reading, Reading::Unjudged) => reading,
            (Reading::Agreed(a), Reading::Agreed(b)) if a == b => Reading::Agreed(a),
            (Reading::Finding | Reading::Agreed(_), Reading::Finding | Reading::Agreed(_)) => {
                Reading::Finding
            }
        }
    }

    pub fn of_rows<'a>(rows: impl IntoIterator<Item = &'a Row>) -> Reading {
        rows.into_iter().fold(Reading::Unjudged, |reading, row| {
            reading.join(Reading::Agreed(row.outcome))
        })
    }
}

/// The words train.sh's verbs read: `ok`, `failed`, `unknown` and `finding`.
impl fmt::Display for Reading {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Reading::Unjudged => "unknown",
            Reading::Agreed(outcome) => outcome.word(),
            Reading::Finding => "finding",
        })
    }
}
