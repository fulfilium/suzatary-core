//! What a run of a half found, and how it ended.

use crate::Error;

/// What a judged run of a half found: the one fact a row of the table stores.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Green,
    Red,
}

impl Outcome {
    /// Its word in a row.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Outcome::Green => "ok",
            Outcome::Red => "failed",
        }
    }

    pub fn parse(word: &str) -> Result<Outcome, Error> {
        [Outcome::Green, Outcome::Red]
            .into_iter()
            .find(|outcome| outcome.word() == word)
            .ok_or_else(|| Error::Malformed(format!("\"{word}\" is neither ok nor failed")))
    }
}

/// How one run of a half ended. Only `Judged` writes a row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ended {
    Judged(Outcome),
    DidNotStart,
}

impl Ended {
    /// The merge gate's exit status: 0 green, 75 "did not start" (merge-gate.sh's own words), and
    /// any other status red, a usage error (64) included.
    #[must_use]
    pub fn of_status(status: u8) -> Ended {
        match status {
            0 => Ended::Judged(Outcome::Green),
            75 => Ended::DidNotStart,
            _ => Ended::Judged(Outcome::Red),
        }
    }

    /// The status `train table write` exits with, which train.sh's callers read: 0 green, 1 red,
    /// 2 no verdict.
    #[must_use]
    pub fn status(self) -> u8 {
        match self {
            Ended::Judged(Outcome::Green) => 0,
            Ended::Judged(Outcome::Red) => 1,
            Ended::DidNotStart => 2,
        }
    }
}
