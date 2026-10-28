//! The box's own clock over what a store serves: a count it keeps and bumps whenever an answer
//! does not extend the last one, so a migration, a drop and a rollback, of the stores or within
//! them, each start a later epoch. A replica orders the box's words by it first, since a drop or a
//! rollback is the box holding less, which no reading of a replica's objects can tell from an
//! older word.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use crate::capability::Datetime;
use crate::refusal::Answered;
use crate::store::{Doc, StoreError};

/// What a store holds at one answer: the prodromes it serves, its heads, and every object it holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Seen {
    /// The geneses of the prodromes the store serves.
    pub geneses: BTreeSet<String>,
    /// Its heads.
    pub heads: BTreeSet<String>,
    /// The name of every object it holds, shared with the store's graph so an answer builds no
    /// set of its own.
    pub held: Arc<BTreeSet<String>>,
}

/// What a store served at its last answer, and the epoch it was served in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Served {
    /// Never smaller than any epoch this box answered with before.
    pub epoch: u64,
    /// The geneses of the prodromes the store served.
    pub geneses: BTreeSet<String>,
    /// The heads it answered with.
    pub heads: BTreeSet<String>,
}

impl Served {
    /// What answering `seen` reads as after `was`. An answer that extends the last (the same
    /// prodromes, every head answered last still held) keeps its epoch; any other, a migration, a
    /// drop or a rollback to an earlier checkpoint of the same stores, starts the next. Every bump
    /// is `max(was + 1, floor)`, `floor` being the box's clock in seconds, so a record lost with
    /// the state directory starts above every epoch it had answered with.
    #[must_use]
    pub fn after(was: Option<&Self>, seen: &Seen, floor: u64) -> Self {
        let epoch = match was {
            Some(was) if was.geneses == seen.geneses && was.heads.is_subset(&seen.held) => {
                was.epoch
            }
            Some(was) => (was.epoch + 1).max(floor),
            None => floor.max(1),
        };
        Self {
            epoch,
            geneses: seen.geneses.clone(),
            heads: seen.heads.clone(),
        }
    }
}

/// Each store's epoch, kept in a [`Doc`] one record per store, each read and written under that
/// store's own lock, so two answers of one store never step from the same record to two different
/// epochs, and a whole store read under one lock holds no other store's answer back.
pub struct Epochs {
    doc: Box<dyn Doc<String, Served> + Send>,
    locks: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
}

impl Epochs {
    /// Epochs kept in `doc`.
    pub fn kept(doc: impl Doc<String, Served> + Send + 'static) -> Self {
        Self {
            doc: Box::new(doc),
            locks: Mutex::default(),
        }
    }

    /// Epochs held in memory, for a server with no state directory of its own, as a test's is.
    #[must_use]
    pub fn remembered() -> Self {
        Self::kept(Held::default())
    }

    /// The epoch of `store` at `now` and what `read` answered beside its holdings, read under the
    /// lock that steps the epoch, so an answer's epoch and its objects are one snapshot; the
    /// record is kept for the next answer.
    ///
    /// # Errors
    /// What `read` answers, and the record's store, unreadable or unwritable.
    pub fn served<T>(
        &self,
        store: &str,
        now: Datetime,
        read: impl FnOnce() -> Answered<(Seen, T)>,
    ) -> Answered<(u64, T)> {
        let lock = self.lock_of(store);
        let _held = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let key = store.to_owned();
        let was = self.doc.get(&key)?;
        let (seen, answer) = read()?;
        let now = Served::after(was.as_ref(), &seen, seconds(now));
        if was.as_ref() != Some(&now) {
            self.doc.set(&key, &now)?;
        }
        Ok((now.epoch, answer))
    }

    /// The lock of `store`, made the first time it is asked for.
    fn lock_of(&self, store: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(locks.entry(store.to_owned()).or_default())
    }
}

/// The records held in memory: the shipped build's own, since `store::InMemoryDoc` is a test's.
#[derive(Default)]
struct Held(Mutex<BTreeMap<String, Served>>);

impl Doc<String, Served> for Held {
    fn get(&self, key: &String) -> Result<Option<Served>, StoreError> {
        Ok(self.held().get(key).cloned())
    }

    fn set(&self, key: &String, value: &Served) -> Result<(), StoreError> {
        self.held().insert(key.clone(), value.clone());
        Ok(())
    }

    fn remove(&self, key: &String) -> Result<(), StoreError> {
        self.held().remove(key);
        Ok(())
    }
}

impl Held {
    fn held(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Served>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Seconds since the epoch of an instant, naive and UTC by definition; none before 1970.
fn seconds(at: Datetime) -> u64 {
    chrono::NaiveDate::from_ymd_opt(at.year(), at.month(), at.day())
        .and_then(|day| day.and_hms_opt(at.hour(), at.minute(), at.second()))
        .and_then(|instant| u64::try_from(instant.and_utc().timestamp()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike as _, Timelike as _};

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|&n| n.to_owned()).collect()
    }

    fn at(seconds: i64) -> Datetime {
        let t = chrono::DateTime::from_timestamp(seconds, 0)
            .expect("an instant")
            .naive_utc();
        Datetime::new(
            t.year(),
            t.month(),
            t.day(),
            t.hour(),
            t.minute(),
            t.second(),
            0,
        )
        .expect("an instant")
    }

    /// A store serving `geneses` whose objects are `held`, the last of them its one head.
    fn seen(geneses: &[&str], held: &[&str]) -> Seen {
        Seen {
            geneses: set(geneses),
            heads: held.last().map(|&h| set(&[h])).unwrap_or_default(),
            held: Arc::new(set(held)),
        }
    }

    fn answer(epochs: &Epochs, store: &str, seen: &Seen, now: i64) -> u64 {
        epochs
            .served(store, at(now), || Ok((seen.clone(), ())))
            .expect("served")
            .0
    }

    #[test]
    fn a_store_set_change_bumps_the_epoch_and_nothing_else_does() {
        let epochs = Epochs::remembered();
        let first = answer(&epochs, "life", &seen(&["a"], &["x"]), 10);
        assert_eq!(
            answer(&epochs, "life", &seen(&["a"], &["x"]), 20),
            first,
            "the same answer, later"
        );
        assert_eq!(
            answer(&epochs, "life", &seen(&["a"], &["x", "y"]), 20),
            first,
            "a write extends it"
        );
        let added = answer(&epochs, "life", &seen(&["a", "b"], &["x", "y"]), 20);
        assert!(added > first, "a migration adds a store");
        let dropped = answer(&epochs, "life", &seen(&["b"], &["x", "y"]), 20);
        assert!(
            dropped > added,
            "a drop is a later epoch, though the box holds less"
        );
        let rolled = answer(&epochs, "life", &seen(&["a", "b"], &["x", "y"]), 5);
        assert!(
            rolled > dropped,
            "a rollback is later still, whatever the clock says"
        );
        assert_eq!(
            answer(&epochs, "store/inbox", &seen(&["a"], &[]), 30),
            30,
            "each store keeps its own"
        );
    }

    /// A store's read runs under its own lock alone: another store answers while it is held,
    /// and a second answer of the same store waits for it.
    #[test]
    fn each_store_steps_under_its_own_lock() {
        let epochs = Epochs::remembered();
        let (entered, leave) = (std::sync::Barrier::new(2), std::sync::Barrier::new(2));
        std::thread::scope(|scope| {
            scope.spawn(|| {
                epochs
                    .served("life", at(10), || {
                        entered.wait();
                        leave.wait();
                        Ok((seen(&["a"], &["x"]), ()))
                    })
                    .expect("served")
            });
            entered.wait();
            assert_eq!(
                answer(&epochs, "store/inbox", &seen(&["b"], &[]), 20),
                20,
                "another store answers while life is read"
            );
            leave.wait();
        });
    }

    #[test]
    fn a_rollback_of_the_same_stores_is_a_later_epoch() {
        let epochs = Epochs::remembered();
        let later = answer(&epochs, "life", &seen(&["a"], &["x", "y"]), 10);
        // A deploy of an earlier checkpoint: the same prodromes, without the head answered last.
        let back = answer(&epochs, "life", &seen(&["a"], &["x"]), 10);
        assert_eq!(back, later + 1);
        assert_eq!(
            answer(&epochs, "life", &seen(&["a"], &["x", "z"]), 10),
            back,
            "and writes after it extend it"
        );
    }

    #[test]
    fn a_lost_record_starts_above_what_it_answered() {
        let was = Served::after(None, &seen(&["a"], &[]), 100);
        let moved = Served::after(Some(&was), &seen(&["b"], &[]), 100);
        assert_eq!(moved.epoch, 101);
        assert!(
            Served::after(None, &seen(&["b"], &[]), 200).epoch > moved.epoch,
            "a later clock over a lost record"
        );
    }
}
