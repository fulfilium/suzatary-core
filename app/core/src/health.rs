//! The health of what the box reads from and calls: a register of results keyed by any source
//! ([`crate::source_key::SourceKey`], and the model each job runs).
//!
//! [`Health`] is a product of semilattices (the latest success, the latest failure, the first
//! result), and [`Healths`] their pointwise join, so a reading is a function of the SET of
//! results, and the store holds the join.

use std::borrow::Borrow;
use std::collections::BTreeMap;

use chrono::{Duration, NaiveDateTime};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::connect::Failure;
use crate::store::StoreError;

/// An instant on the box's clock, naive and UTC, as every `at` is.
pub type Instant = NaiveDateTime;

/// What one result came to: a success, or a failure of a class the box parsed where it was
/// observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// It succeeded.
    Succeeded,
    /// It failed, of this class.
    Failed(Failure),
}

/// One failure: when, its class, and the last line the account's sync said.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Failed {
    /// When the sync failed.
    pub at: Instant,
    /// Its class; ordered after `at`, so two failures at one instant still join to one.
    pub class: Failure,
    /// What it said.
    pub words: String,
}

/// One account's sync results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Stored", into = "Stored")]
pub struct Health {
    results: Results,
    /// The latest failure of a class a tap mends: what a fix answers while an outage or a
    /// timeout comes between two sightings of it.
    classified: Option<Failed>,
    /// The first result recorded, success or not.
    first: Instant,
}

/// The latest success and failure, at least one.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Results {
    Succeeded(Instant, Option<Failed>),
    Failed(Failed),
}

impl Results {
    fn join(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Failed(one), Self::Failed(two)) => Self::Failed(one.max(two).clone()),
            (Self::Succeeded(ok, failed), Self::Failed(other))
            | (Self::Failed(other), Self::Succeeded(ok, failed)) => {
                Self::Succeeded(*ok, failed.clone().max(Some(other.clone())))
            }
            (Self::Succeeded(one, failed), Self::Succeeded(two, other)) => {
                Self::Succeeded(*one.max(two), failed.clone().max(other.clone()))
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    ok: Option<Instant>,
    failed: Option<Failed>,
    classified: Option<Failed>,
    first: Instant,
}

impl TryFrom<Stored> for Health {
    type Error = &'static str;

    fn try_from(stored: Stored) -> Result<Self, Self::Error> {
        let results = match (stored.ok, stored.failed) {
            (Some(ok), failed) => Results::Succeeded(ok, failed),
            (None, Some(failed)) => Results::Failed(failed),
            (None, None) => return Err("a health holds neither a success nor a failure"),
        };
        Ok(Self {
            results,
            classified: stored.classified,
            first: stored.first,
        })
    }
}

impl From<Health> for Stored {
    fn from(health: Health) -> Self {
        let (ok, failed) = match health.results {
            Results::Succeeded(ok, failed) => (Some(ok), failed),
            Results::Failed(failed) => (None, Some(failed)),
        };
        Self {
            ok,
            failed,
            classified: health.classified,
            first: health.first,
        }
    }
}

impl Health {
    /// One result at `at`; `words` is what it said.
    #[must_use]
    pub fn of(at: Instant, outcome: Outcome, words: &str) -> Self {
        let results = match outcome {
            Outcome::Succeeded => Results::Succeeded(at, None),
            Outcome::Failed(class) => Results::Failed(Failed {
                at,
                class,
                words: words.to_owned(),
            }),
        };
        let classified = match &results {
            Results::Failed(failed) => Some(failed.clone()),
            Results::Succeeded(..) => None,
        }
        .filter(|failed| failed.class.remedy() != crate::connect::Remedy::Retry);
        Self {
            results,
            classified,
            first: at,
        }
    }

    /// The join: commutative, associative and idempotent, each part by its own order.
    #[must_use]
    pub fn join(&self, other: &Self) -> Self {
        Self {
            results: self.results.join(&other.results),
            classified: self.classified.clone().max(other.classified.clone()),
            first: self.first.min(other.first),
        }
    }

    /// Its latest success, `None` while none is recorded.
    #[must_use]
    pub const fn succeeded(&self) -> Option<Instant> {
        match self.results {
            Results::Succeeded(ok, _) => Some(ok),
            Results::Failed(_) => None,
        }
    }

    /// While failing, the class a fix answers and since when: the latest failure since the last
    /// success whose class a tap mends, else the latest failure's.
    #[must_use]
    pub fn remedial(&self) -> Option<(Failure, Instant)> {
        match self.reading() {
            Reading::Ok { .. } => None,
            Reading::Failing { class, since, .. } => Some((
                self.classified
                    .as_ref()
                    .filter(|classified| self.succeeded().is_none_or(|ok| classified.at > ok))
                    .map_or(class, |classified| classified.class),
                since,
            )),
        }
    }

    /// What the account's results say now. A success at the instant of a failure clears it.
    #[must_use]
    pub fn reading(&self) -> Reading {
        let failing = |failed: &Failed, since: Instant| Reading::Failing {
            at: failed.at,
            class: failed.class,
            words: failed.words.clone(),
            since,
        };
        match &self.results {
            Results::Failed(failed) => failing(failed, self.first),
            Results::Succeeded(ok, Some(failed)) if failed.at > *ok => failing(failed, *ok),
            Results::Succeeded(ok, _) => Reading::Ok { at: *ok },
        }
    }
}

/// What an account's sync results say now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "MailHealth")]
pub enum Reading {
    /// Its latest sync succeeded.
    Ok {
        /// When.
        #[ts(type = "string")]
        at: Instant,
    },
    /// Its latest sync failed.
    Failing {
        /// When.
        #[ts(type = "string")]
        at: Instant,
        /// Its class.
        class: Failure,
        /// The last line the sync said.
        words: String,
        /// It has not synced since this instant: its last success or, with none recorded, its
        /// first result.
        #[ts(type = "string")]
        since: Instant,
    },
}

impl Reading {
    /// How long it has been failing at `now`, `None` when its latest sync succeeded.
    #[must_use]
    pub fn failing_for(&self, now: Instant) -> Option<Duration> {
        match self {
            Reading::Ok { .. } => None,
            Reading::Failing { since, .. } => Some(now - *since),
        }
    }
}

/// Every source's health, joined pointwise: a source in one side only is kept as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Healths<K: Ord>(BTreeMap<K, Health>);

impl<K: Ord> Default for Healths<K> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K: Ord + Clone> Healths<K> {
    /// One source's one result: the generator every register is a join of.
    #[must_use]
    pub fn one(source: impl Into<K>, at: Instant, outcome: Outcome, words: &str) -> Self {
        Self(BTreeMap::from([(
            source.into(),
            Health::of(at, outcome, words),
        )]))
    }

    /// The pointwise join.
    #[must_use]
    pub fn join(&self, other: &Self) -> Self {
        let mut joined = self.0.clone();
        for (source, health) in &other.0 {
            joined
                .entry(source.clone())
                .and_modify(|mine| *mine = mine.join(health))
                .or_insert_with(|| health.clone());
        }
        Self(joined)
    }

    /// One source's reading, `None` for a source nothing has reported.
    #[must_use]
    pub fn reading<Q: Ord + ?Sized>(&self, source: &Q) -> Option<Reading>
    where
        K: Borrow<Q>,
    {
        self.health(source).map(Health::reading)
    }

    /// One source's joined results, `None` for a source nothing has reported.
    #[must_use]
    pub fn health<Q: Ord + ?Sized>(&self, source: &Q) -> Option<&Health>
    where
        K: Borrow<Q>,
    {
        self.0.get(source)
    }

    /// The latest success of any source, `None` while none has succeeded.
    #[must_use]
    pub fn succeeded(&self) -> Option<Instant> {
        self.0.values().filter_map(Health::succeeded).max()
    }

    /// Every source's reading.
    pub fn readings(&self) -> impl Iterator<Item = (&K, Reading)> {
        self.0
            .iter()
            .map(|(source, health)| (source, health.reading()))
    }
}

/// A register of results, read whole: what a reader holds.
pub trait Register<K: Ord>: Sync {
    /// What it holds; empty before the first result.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn healths(&self) -> Result<Healths<K>, StoreError>;
}

/// A register held in memory, as a test's server holds one.
impl<K: Ord + Clone + Send> Register<K> for std::sync::Mutex<Healths<K>> {
    fn healths(&self) -> Result<Healths<K>, StoreError> {
        Ok(self
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone())
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn instant(minutes: i64) -> Instant {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 1)
            .and_then(|day| day.and_hms_opt(0, 0, 0))
            .expect("a real instant")
            + Duration::minutes(minutes)
    }

    /// A failure's class and words, `None` for a success.
    type Said = Option<(Failure, String)>;

    /// A result: an account of three, a minute, and a class and words when it failed.
    fn result() -> impl Strategy<Value = (usize, i64, Said)> {
        (
            0..3usize,
            0..10_000i64,
            proptest::option::of((
                proptest::sample::select(Failure::ALL.to_vec()),
                "[a-c]{1,2}",
            )),
        )
    }

    fn outcome(said: &Said) -> (Outcome, &str) {
        match said {
            None => (Outcome::Succeeded, ""),
            Some((class, words)) => (Outcome::Failed(*class), words.as_str()),
        }
    }

    fn healths(results: &[(usize, i64, Said)]) -> Healths<String> {
        results
            .iter()
            .map(|(account, minute, said)| {
                let (outcome, words) = outcome(said);
                Healths::one(
                    format!("a{account}@example.test"),
                    instant(*minute),
                    outcome,
                    words,
                )
            })
            .fold(Healths::default(), |sum, one| sum.join(&one))
    }

    /// The meaning: the latest result decides, a success winning a tie, and a failure has held
    /// since the latest success or the first result.
    fn meaning(results: &[(usize, i64, Said)], account: usize) -> Option<Reading> {
        let mine: Vec<(Instant, &Said)> = results
            .iter()
            .filter(|(which, _, _)| *which == account)
            .map(|(_, minute, said)| (instant(*minute), said))
            .collect();
        let first = mine.iter().map(|(at, _)| *at).min()?;
        let ok = mine
            .iter()
            .filter(|(_, said)| said.is_none())
            .map(|(at, _)| *at)
            .max();
        let failed = mine
            .iter()
            .filter_map(|(at, said)| said.as_ref().map(|(class, words)| (*at, *class, words)))
            .max();
        Some(match failed {
            Some((at, class, words)) if ok.is_none_or(|ok| at > ok) => Reading::Failing {
                at,
                class,
                words: words.clone(),
                since: ok.unwrap_or(first),
            },
            _ => Reading::Ok {
                at: ok.expect("a result that is not a failure is a success"),
            },
        })
    }

    proptest! {
        /// The register reads as its meaning, over any set of results in any order.
        #[test]
        fn the_reading_is_the_latest_result_and_how_long_it_has_held(
            results in proptest::collection::vec(result(), 0..24),
        ) {
            let joined = healths(&results);
            for account in 0..3 {
                prop_assert_eq!(
                    joined.reading(&format!("a{account}@example.test")),
                    meaning(&results, account)
                );
            }
        }

        /// The join is a semilattice's, so the stored join is a function of the set of results:
        /// order, duplication and a split into two stores read the same.
        #[test]
        fn the_join_is_commutative_associative_and_idempotent(
            a in proptest::collection::vec(result(), 0..8),
            b in proptest::collection::vec(result(), 0..8),
            c in proptest::collection::vec(result(), 0..8),
        ) {
            let (a, b, c) = (healths(&a), healths(&b), healths(&c));
            prop_assert_eq!(a.join(&b), b.join(&a));
            prop_assert_eq!(a.join(&b).join(&c), a.join(&b.join(&c)));
            prop_assert_eq!(a.join(&a), a.clone());
            prop_assert_eq!(Healths::default().join(&a), a);
        }
    }

    /// While failing, a fix answers the latest failure a tap mends since the last success, so
    /// an outage between two refusals keeps the refusal's class; one before the success does not.
    #[test]
    fn a_fix_answers_the_episodes_latest_failure_a_tap_mends() {
        let refused = Outcome::Failed(Failure::NeedsLogin);
        let outage = Outcome::Failed(Failure::Unclassified);
        let health = Health::of(instant(0), Outcome::Succeeded, "")
            .join(&Health::of(instant(10), refused, "lapsed"))
            .join(&Health::of(instant(20), outage, "timed out"));
        assert_eq!(health.remedial(), Some((Failure::NeedsLogin, instant(0))));
        let after = health
            .join(&Health::of(instant(30), Outcome::Succeeded, ""))
            .join(&Health::of(instant(40), outage, "timed out"));
        assert_eq!(after.remedial(), Some((Failure::Unclassified, instant(30))));
        assert_eq!(
            after
                .join(&Health::of(instant(50), Outcome::Succeeded, ""))
                .remedial(),
            None
        );
    }
}
