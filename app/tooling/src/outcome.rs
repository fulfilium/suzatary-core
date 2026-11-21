//! What a run of a half found, and how it ended.

use crate::Error;
use crate::supervisor::Status;

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

/// How a half ended. Only `Judged` writes a row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ended {
    Judged(Outcome),
    /// It says nothing of the tree: its gate did not start (75), its run on the runners was lost,
    /// cancelled or never came, or it was stopped.
    Unjudged,
    /// The one check before a push kept the tree on the box, so nothing judged it there.
    Refused(Check),
}

/// Why the one check before a push to core (`train publishable`, `push-gate`) kept a tree on the
/// box.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Check {
    Leak,
    Held,
    Unrun,
}

impl Ended {
    /// The merge gate's end: 0 green, 75 "did not start" (merge-gate.sh's own words), any other
    /// status red, a usage error (64) included; and a status nothing read says nothing.
    #[must_use]
    pub fn of(status: Status) -> Ended {
        match status {
            Status::Read(0) => Ended::Judged(Outcome::Green),
            Status::Read(75) | Status::Unread => Ended::Unjudged,
            Status::Read(_) => Ended::Judged(Outcome::Red),
        }
    }
}

impl Check {
    /// `train publishable`'s and `push-gate`'s status (`Publishable::status`): 0 passes, and a
    /// status nothing read is a check that could not run.
    pub fn of(status: Status) -> Result<(), Check> {
        match status {
            Status::Read(0) => Ok(()),
            Status::Read(1) => Err(Check::Leak),
            Status::Read(4) => Err(Check::Held),
            Status::Read(_) | Status::Unread => Err(Check::Unrun),
        }
    }
}
