//! Each mail account's sync health: the register the timer's syncs write and the settings page,
//! the briefing and the inbox read. The register is keyed by any source, so whether the model each
//! job runs answers is the same semilattice ([`Healths::one`]).
//!
//! What is read of an account's sync results is three things: the latest success, the latest
//! failure with its words, and the first result recorded. Each is a semilattice (a maximum, a
//! maximum by instant, a minimum), so their product [`Health`] is one, and so is [`Healths`],
//! the pointwise join over accounts. The reading is therefore a function of the SET of results,
//! whatever order they arrive in and however often, and the store holds the join rather than
//! every result: joining one more result into it is adding that result to the set.

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDateTime};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::connect::Failure;
use crate::mail::verbs::SyncReport;
use crate::store::{Doc, StoreError};

/// An instant on the box's clock, naive and UTC, as every `at` is.
pub type Instant = NaiveDateTime;

/// How long an account may fail before the box proposes a todo about it.
#[must_use]
pub fn persistent() -> Duration {
    Duration::days(1)
}

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

/// One account's sync results, as the three semilattices they are read through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Health {
    /// The latest success.
    ok: Option<Instant>,
    /// The latest failure.
    failed: Option<Failed>,
    /// The latest failure of a class a tap mends: what a fix answers while an outage or a
    /// timeout comes between two sightings of it.
    classified: Option<Failed>,
    /// The first result recorded, success or not.
    first: Instant,
}

impl Health {
    /// The health of one result at `at`; `words` is what it said, kept for a failure.
    #[must_use]
    pub fn of(at: Instant, outcome: Outcome, words: &str) -> Self {
        let (ok, failed) = match outcome {
            Outcome::Succeeded => (Some(at), None),
            Outcome::Failed(class) => (
                None,
                Some(Failed {
                    at,
                    class,
                    words: words.to_owned(),
                }),
            ),
        };
        let classified = failed
            .clone()
            .filter(|failed| failed.class.remedy() != crate::connect::Remedy::Retry);
        Self {
            ok,
            failed,
            classified,
            first: at,
        }
    }

    /// The join: commutative, associative and idempotent, each part by its own order.
    #[must_use]
    pub fn join(&self, other: &Self) -> Self {
        Self {
            ok: self.ok.max(other.ok),
            failed: self.failed.clone().max(other.failed.clone()),
            classified: self.classified.clone().max(other.classified.clone()),
            first: self.first.min(other.first),
        }
    }

    /// Its latest success, `None` while none is recorded.
    #[must_use]
    pub const fn succeeded(&self) -> Option<Instant> {
        self.ok
    }

    /// While failing, the class a fix answers and since when: the latest failure since the last
    /// success whose class a tap mends, else the latest failure's. So a revoked link stays
    /// revoked through a timeout between two of its 403s, and its card stays the same card.
    #[must_use]
    pub fn remedial(&self) -> Option<(Failure, Instant)> {
        match self.reading() {
            Reading::Ok { .. } => None,
            Reading::Failing { class, since, .. } => Some((
                self.classified
                    .as_ref()
                    .filter(|classified| self.ok.is_none_or(|ok| classified.at > ok))
                    .map_or(class, |classified| classified.class),
                since,
            )),
        }
    }

    /// What the account's results say now. A success at the instant of a failure clears it.
    #[must_use]
    pub fn reading(&self) -> Reading {
        match (&self.failed, self.ok) {
            (Some(failed), ok) if ok.is_none_or(|ok| failed.at > ok) => Reading::Failing {
                at: failed.at,
                class: failed.class,
                words: failed.words.clone(),
                since: ok.unwrap_or(self.first),
            },
            (_, Some(at)) => Reading::Ok { at },
            // Reached only by a stored health holding neither, which no join writes: read as
            // having said nothing against the account.
            (_, None) => Reading::Ok { at: self.first },
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

/// Every account's health, joined pointwise: an account in one side only is kept as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Healths(BTreeMap<String, Health>);

impl Healths {
    /// One source's one result, at `at`, `words` what it said: the generator every register is
    /// a join of, whatever the source (a mail account, the model a job runs, a plugin's link).
    #[must_use]
    pub fn one(source: &str, at: Instant, outcome: Outcome, words: &str) -> Self {
        Self(BTreeMap::from([(
            source.to_owned(),
            Health::of(at, outcome, words),
        )]))
    }

    /// The health one sync's report gives, every account at `at`.
    #[must_use]
    pub fn of(report: &SyncReport, at: Instant) -> Self {
        report
            .synced
            .iter()
            .map(|account| Self::one(&account.account, at, account.outcome, &account.tail))
            .fold(Self::default(), |sum, one| sum.join(&one))
    }

    /// The pointwise join.
    #[must_use]
    pub fn join(&self, other: &Self) -> Self {
        let mut joined = self.0.clone();
        for (account, health) in &other.0 {
            joined
                .entry(account.clone())
                .and_modify(|mine| *mine = mine.join(health))
                .or_insert_with(|| health.clone());
        }
        Self(joined)
    }

    /// One account's reading, `None` for an account no sync has reported.
    #[must_use]
    pub fn reading(&self, account: &str) -> Option<Reading> {
        self.health(account).map(Health::reading)
    }

    /// One source's joined results, `None` for a source nothing has reported.
    #[must_use]
    pub fn health(&self, source: &str) -> Option<&Health> {
        self.0
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(source))
            .map(|(_, health)| health)
    }

    /// The latest success of any source, `None` while none has succeeded.
    #[must_use]
    pub fn succeeded(&self) -> Option<Instant> {
        self.0.values().filter_map(Health::succeeded).max()
    }

    /// Every account's reading, by address.
    pub fn readings(&self) -> impl Iterator<Item = (&str, Reading)> {
        self.0
            .iter()
            .map(|(account, health)| (account.as_str(), health.reading()))
    }
}

/// Join one sync's report, taken at `at`, into the register and answer what it holds now.
///
/// # Errors
/// The store's, reading or writing; a sync whose health is not recorded says so.
pub fn record(
    store: &dyn Doc<(), Healths>,
    report: &SyncReport,
    at: Instant,
) -> Result<Healths, StoreError> {
    join_into(store, &Healths::of(report, at))
}

/// Join `results` into the register and answer what it holds now.
///
/// # Errors
/// The store's, reading or writing.
pub fn join_into(store: &dyn Doc<(), Healths>, results: &Healths) -> Result<Healths, StoreError> {
    let joined = store.get(&())?.unwrap_or_default().join(results);
    store.set(&(), &joined)?;
    Ok(joined)
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

    fn healths(results: &[(usize, i64, Said)]) -> Healths {
        results
            .iter()
            .map(|(account, minute, said)| {
                let (outcome, words) = outcome(said);
                Healths::one(
                    &format!("a{account}@example.test"),
                    instant(*minute),
                    outcome,
                    words,
                )
            })
            .fold(Healths::default(), |sum, one| sum.join(&one))
    }

    /// The meaning, written out plainly: the latest result decides, a success at a failure's
    /// instant winning, and a failure has held since the latest success or the first result.
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
