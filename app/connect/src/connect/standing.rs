//! Where a line of the overview, and so a connection, stands.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::secrets::SecretState;
use crate::settings::Help;
use suzatary_health::health::{Instant, Reading};

/// Where a line stands; the arms are ordered by how much they ask of the owner, which
/// [`Standing::join`] reads, so do not reorder them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "LineStanding")]
pub enum Standing {
    /// In place, with nothing to observe: the data repository, this box.
    Kept,
    Synced {
        /// ISO 8601, UTC.
        at: String,
    },
    Answered {
        /// ISO 8601, UTC.
        at: String,
    },
    Unobserved,
    Locked,
    StoreFailed,
    Unreadable {
        words: String,
    },
    Failing {
        /// ISO 8601, UTC: the instant since which it has not succeeded.
        since: String,
        class: suzatary_health::health::Failure,
        words: String,
        /// ISO 8601, UTC.
        at: String,
    },
    /// A step only a terminal on the box takes; it ranks above a failure because a sync fails
    /// until the step is taken.
    ByHand {
        /// Its place among its connection's steps.
        #[serde(skip)]
        #[ts(skip)]
        step: usize,
        command: String,
        why: String,
    },
    /// Secrets it reads are stored nowhere on this box, each in the owner's words.
    Missing {
        /// Sorted.
        what: Vec<String>,
    },
}

impl Standing {
    #[must_use]
    pub fn asks(&self) -> bool {
        match self {
            Standing::Kept
            | Standing::Synced { .. }
            | Standing::Answered { .. }
            | Standing::Unobserved => false,
            Standing::Locked
            | Standing::StoreFailed
            | Standing::Unreadable { .. }
            | Standing::Failing { .. }
            | Standing::ByHand { .. }
            | Standing::Missing { .. } => true,
        }
    }

    /// The part that asks more of the owner; two missing parts merge and of two steps by hand the
    /// earlier wins: a join-semilattice whose identity is [`Standing::Kept`].
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Missing { what: mut mine }, Self::Missing { what: theirs }) => {
                mine.extend(theirs);
                mine.sort();
                mine.dedup();
                Self::Missing { what: mine }
            }
            (mine @ Self::ByHand { .. }, theirs @ Self::ByHand { .. }) => mine.min(theirs),
            (mine, theirs) => mine.max(theirs),
        }
    }

    #[must_use]
    pub(crate) fn at_step(self, step: usize) -> Self {
        match self {
            Self::ByHand { command, why, .. } => Self::ByHand { step, command, why },
            other => other,
        }
    }

    #[must_use]
    pub fn missing(what: impl Into<String>) -> Self {
        Self::Missing {
            what: vec![what.into()],
        }
    }

    #[must_use]
    pub fn of_secret(name: &str, state: SecretState) -> Self {
        match state {
            SecretState::Set => Self::Kept,
            SecretState::Missing => Self::missing(Help::label(name)),
            SecretState::Locked => Self::Locked,
            SecretState::Failed => Self::StoreFailed,
        }
    }

    #[must_use]
    pub fn of_health(reading: Option<Reading>) -> Self {
        match reading {
            None => Self::Unobserved,
            Some(Reading::Ok { at }) => Self::Synced { at: stamp(at) },
            Some(Reading::Failing {
                at,
                class,
                words,
                since,
            }) => Self::Failing {
                since: stamp(since),
                class,
                words,
                at: stamp(at),
            },
        }
    }

    /// As [`Standing::of_health`], with a success read as an answer.
    #[must_use]
    pub fn of_calls(reading: Option<Reading>) -> Self {
        match Self::of_health(reading) {
            Self::Synced { at } => Self::Answered { at },
            other => other,
        }
    }
}

fn stamp(at: Instant) -> String {
    at.format("%Y-%m-%dT%H:%M:%S%.f").to_string()
}
