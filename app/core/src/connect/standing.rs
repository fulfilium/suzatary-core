//! Where a line of the overview, and so a connection, stands.

use serde::Serialize;
use ts_rs::TS;

use crate::capability::SecretState;
use crate::health::{Instant, Reading};
use crate::settings::Help;

/// Where a line stands, ordered by how much it asks of the owner (the arms in that order), so
/// [`Standing::join`] of a line's parts is the one that asks most; of two missing, both, and of
/// two steps by hand, the earlier. A connection's standing is
/// the join of its steps' ([`super::Connection::standing`]) and its check's results, never kept,
/// so a restart, a second device and a second conversation read the same answer.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "LineStanding")]
pub enum Standing {
    /// In place, with nothing to observe: the data repository, this box.
    Kept,
    /// Its latest sync succeeded, at this instant.
    Synced {
        /// When, ISO 8601, UTC.
        at: String,
    },
    /// Its model's latest call answered, at this instant.
    Answered {
        /// When, ISO 8601, UTC.
        at: String,
    },
    /// In place, and nothing has observed it yet.
    Unobserved,
    /// The secret store is locked, which an unlock at the box mends.
    Locked,
    /// The secret store could not be run.
    StoreFailed,
    /// What the box keeps of it could not be read.
    Unreadable {
        /// Why, in words.
        words: String,
    },
    /// Its latest observation failed.
    Failing {
        /// It has not succeeded since this instant, ISO 8601, UTC.
        since: String,
        /// Its class, which names its one fix.
        class: super::Failure,
        /// What the failure said, in words.
        words: String,
        /// When it last tried, ISO 8601, UTC.
        at: String,
    },
    /// A step only a terminal on the box takes, which the box sees taken only once its check
    /// first succeeds; above a failure, since a sync fails until the step is taken. Of two, the
    /// earlier in its template's order is the one to take first.
    ByHand {
        /// Its place among its connection's steps.
        #[serde(skip)]
        #[ts(skip)]
        step: usize,
        /// What to run, in the owner's words where it sets a secret.
        command: String,
        /// Why, in a few words.
        why: String,
    },
    /// Secrets it reads are stored nowhere on this box, each in the owner's words.
    Missing {
        /// What is missing, sorted.
        what: Vec<String>,
    },
}

impl Standing {
    /// The part that asks more of the owner, and two missing parts missing both: a
    /// join-semilattice whose identity is [`Standing::Kept`].
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

    /// The same standing, a step only a terminal takes placed `step`-th among its connection's.
    #[must_use]
    pub(crate) fn at_step(self, step: usize) -> Self {
        match self {
            Self::ByHand { command, why, .. } => Self::ByHand { step, command, why },
            other => other,
        }
    }

    /// Missing one thing, said in the owner's words.
    #[must_use]
    pub fn missing(what: impl Into<String>) -> Self {
        Self::Missing {
            what: vec![what.into()],
        }
    }

    /// Where a secret named `name` stands, missing in the owner's words.
    #[must_use]
    pub fn of_secret(name: &str, state: SecretState) -> Self {
        match state {
            SecretState::Set => Self::Kept,
            SecretState::Missing => Self::missing(Help::label(name)),
            SecretState::Locked => Self::Locked,
            SecretState::Failed => Self::StoreFailed,
        }
    }

    /// What a sync's health register reads: its latest result, or nothing observed yet.
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

    /// What a model's calls read: as a sync's, its success an answer.
    #[must_use]
    pub fn of_calls(reading: Option<Reading>) -> Self {
        match Self::of_health(reading) {
            Self::Synced { at } => Self::Answered { at },
            other => other,
        }
    }
}

/// An instant as the wire spells one.
fn stamp(at: Instant) -> String {
    at.format("%Y-%m-%dT%H:%M:%S%.f").to_string()
}
