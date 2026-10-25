//! A replica's frontier named in 64 characters, however many heads it has. Every answer to a
//! pull ends at a frontier the box computes, the heads of what the asker holds after taking it;
//! the box records that set under its digest and sends the digest, and the asker names it back
//! as `since`. A digest the box no longer holds reads as unknown, and the box then answers as to
//! a replica holding nothing, so no answer ever leaves out an object the asker lacks.
//!
//! Set reconciliation over a store of the Prodrome's, and nothing of this deployment's: it reads
//! only `prodrome_core`'s names, a [`Dag`](prodrome_core::dag::Dag) and its
//! [`struct@Hash`]es, and keeps its records through [`Records`], its own effect, so it is
//! `prodrome_core::store::frontier`-in-waiting, beside `store::replica`'s join of two replicas.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use prodrome_core::event::Hash;
use sha2::Digest as _;

/// The sha256 of a set of heads, each name on a line of its own in sorted order: 64 lowercase
/// hex digits.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest(String);

/// A `since` that is not 64 lowercase hex digits.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("since is a frontier of 64 hex digits")]
pub struct NotADigest;

impl Digest {
    /// The digest of `heads`.
    #[must_use]
    pub fn of(heads: &BTreeSet<Hash>) -> Self {
        let mut hasher = sha2::Sha256::new();
        for head in heads {
            hasher.update(head.as_str().as_bytes());
            hasher.update(b"\n");
        }
        Self(hex::encode(hasher.finalize()))
    }

    /// `text` as a digest.
    ///
    /// # Errors
    /// [`NotADigest`] for anything but 64 lowercase hex digits.
    pub fn parse(text: &str) -> Result<Self, NotADigest> {
        let hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
        if text.len() == 64 && text.bytes().all(hex) {
            Ok(Self(text.to_owned()))
        } else {
            Err(NotADigest)
        }
    }

    /// The 64 hex digits.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a pull says the asker holds: nothing, or the frontier an earlier answer named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Since {
    /// A replica holding nothing of the store.
    Nothing,
    /// The frontier an answer of this box named.
    Frontier(Digest),
}

/// A `since` as the box reads it: the heads it names, or a digest it does not hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Read {
    /// The heads named, none for a replica holding nothing.
    Known(BTreeSet<Hash>),
    /// A digest this box never recorded, or let go.
    Unknown,
}

/// The records could not be read or written, in their own words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the recorded frontiers: {0}")]
pub struct Unrecorded(pub String);

/// Where the frontiers are kept: each set of heads by its store and digest, and each store's
/// digests, newest last. Every method is one read or one replacement, never half of one.
pub trait Records: Send + Sync {
    /// The heads recorded under `digest` in `store`, or `None`.
    ///
    /// # Errors
    /// [`Unrecorded`], the records unreadable.
    fn heads(&self, store: &str, digest: &Digest) -> Result<Option<BTreeSet<Hash>>, Unrecorded>;

    /// `heads` recorded under `digest` in `store`.
    ///
    /// # Errors
    /// [`Unrecorded`], the records unwritable.
    fn keep(&self, store: &str, digest: &Digest, heads: &BTreeSet<Hash>) -> Result<(), Unrecorded>;

    /// The heads under `digest` in `store` let go; letting go what is not there succeeds.
    ///
    /// # Errors
    /// [`Unrecorded`], the records unwritable.
    fn let_go(&self, store: &str, digest: &Digest) -> Result<(), Unrecorded>;

    /// The digests `store` keeps, newest last; none when it keeps none.
    ///
    /// # Errors
    /// [`Unrecorded`], the records unreadable.
    fn ring(&self, store: &str) -> Result<Vec<Digest>, Unrecorded>;

    /// `ring` as the digests `store` keeps.
    ///
    /// # Errors
    /// [`Unrecorded`], the records unwritable.
    fn keep_ring(&self, store: &str, ring: &[Digest]) -> Result<(), Unrecorded>;
}

/// How many frontiers a store keeps, the most recently answered: a replica whose frontier went
/// before them is answered as one holding nothing, once.
pub const KEPT: usize = 512;

/// Each store's recorded frontiers, kept in [`Records`], at most [`KEPT`] of them. A store's
/// records are read and written under its own lock.
pub struct Frontiers {
    records: Box<dyn Records>,
    locks: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
}

impl Frontiers {
    /// Frontiers kept in `records`.
    pub fn over(records: impl Records + 'static) -> Self {
        Self {
            records: Box::new(records),
            locks: Mutex::default(),
        }
    }

    /// Frontiers held in memory, for a server with no state directory of its own.
    #[must_use]
    pub fn remembered() -> Self {
        Self::over(Remembered::default())
    }

    /// What `since` names in `store`.
    ///
    /// # Errors
    /// [`Unrecorded`].
    pub fn read(&self, store: &str, since: &Since) -> Result<Read, Unrecorded> {
        match since {
            Since::Nothing => Ok(Read::Known(BTreeSet::new())),
            Since::Frontier(digest) => Ok(self
                .records
                .heads(store, digest)?
                .map_or(Read::Unknown, Read::Known)),
        }
    }

    /// What `since` reaches in `dag`, a store's objects now: the closure of the heads it names
    /// when `dag` holds every one, and unknown otherwise, so a frontier recorded before the store
    /// went back (a later epoch holding less) is answered as from nothing rather than read as
    /// reaching only what survived, which would name a frontier the asker passed long ago.
    ///
    /// # Errors
    /// [`Unrecorded`].
    pub fn reached<E: prodrome_core::schema::Schema>(
        &self,
        store: &str,
        since: &Since,
        dag: &prodrome_core::dag::Dag<E>,
    ) -> Result<Read, Unrecorded> {
        Ok(match self.read(store, since)? {
            Read::Known(heads) if heads.iter().all(|head| dag.get(head).is_some()) => {
                Read::Known(dag.closure(heads))
            }
            Read::Known(_) | Read::Unknown => Read::Unknown,
        })
    }

    /// `heads` recorded in `store` as its newest, and their digest; a set recorded before is
    /// moved to the newest and written once. The oldest past [`KEPT`] is let go.
    ///
    /// # Errors
    /// [`Unrecorded`].
    pub fn record(&self, store: &str, heads: &BTreeSet<Hash>) -> Result<Digest, Unrecorded> {
        let lock = self.lock_of(store);
        let _held = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let digest = Digest::of(heads);
        let mut ring = self.records.ring(store)?;
        let known = ring.contains(&digest);
        if known && ring.last() == Some(&digest) {
            return Ok(digest);
        }
        if known {
            ring.retain(|kept| kept != &digest);
        } else {
            self.records.keep(store, &digest, heads)?;
        }
        ring.push(digest.clone());
        let gone = ring.len().saturating_sub(KEPT);
        for old in ring.drain(..gone) {
            self.records.let_go(store, &old)?;
        }
        self.records.keep_ring(store, &ring)?;
        Ok(digest)
    }

    fn lock_of(&self, store: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(locks.entry(store.to_owned()).or_default())
    }
}

/// Records held in memory.
#[derive(Default)]
struct Remembered {
    sets: Mutex<BTreeMap<(String, Digest), BTreeSet<Hash>>>,
    rings: Mutex<BTreeMap<String, Vec<Digest>>>,
}

impl Records for Remembered {
    fn heads(&self, store: &str, digest: &Digest) -> Result<Option<BTreeSet<Hash>>, Unrecorded> {
        Ok(held(&self.sets)
            .get(&(store.to_owned(), digest.clone()))
            .cloned())
    }

    fn keep(&self, store: &str, digest: &Digest, heads: &BTreeSet<Hash>) -> Result<(), Unrecorded> {
        held(&self.sets).insert((store.to_owned(), digest.clone()), heads.clone());
        Ok(())
    }

    fn let_go(&self, store: &str, digest: &Digest) -> Result<(), Unrecorded> {
        held(&self.sets).remove(&(store.to_owned(), digest.clone()));
        Ok(())
    }

    fn ring(&self, store: &str) -> Result<Vec<Digest>, Unrecorded> {
        Ok(held(&self.rings).get(store).cloned().unwrap_or_default())
    }

    fn keep_ring(&self, store: &str, ring: &[Digest]) -> Result<(), Unrecorded> {
        held(&self.rings).insert(store.to_owned(), ring.to_vec());
        Ok(())
    }
}

fn held<T>(map: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    map.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(n: usize) -> Hash {
        Hash::new(format!("{n:064x}")).expect("a name")
    }

    fn set(names: &[usize]) -> BTreeSet<Hash> {
        names.iter().map(|&n| head(n)).collect()
    }

    /// A frontier reads back as the set recorded, by its digest alone, until [`KEPT`] newer ones
    /// push it out; one asked again is the newest again, and an unknown digest says so.
    #[test]
    fn a_recorded_frontier_reads_back_until_newer_ones_push_it_out() {
        let frontiers = Frontiers::remembered();
        let first = frontiers
            .record("store/lines", &set(&[1, 2]))
            .expect("kept");
        assert_eq!(
            frontiers
                .read("store/lines", &Since::Frontier(first.clone()))
                .expect("reads"),
            Read::Known(set(&[1, 2]))
        );
        assert_eq!(
            frontiers
                .read("store/inbox", &Since::Frontier(first.clone()))
                .expect("reads"),
            Read::Unknown,
            "a frontier is its store's"
        );
        for n in 0..KEPT - 1 {
            frontiers
                .record("store/lines", &set(&[100 + n]))
                .expect("kept");
        }
        assert_eq!(
            frontiers
                .record("store/lines", &set(&[1, 2]))
                .expect("kept"),
            first,
            "the same set has the same digest"
        );
        frontiers
            .record("store/lines", &set(&[99_999]))
            .expect("kept");
        assert!(matches!(
            frontiers
                .read("store/lines", &Since::Frontier(first.clone()))
                .expect("reads"),
            Read::Known(_)
        ));
        for n in 0..KEPT {
            frontiers
                .record("store/lines", &set(&[200_000 + n]))
                .expect("kept");
        }
        assert_eq!(
            frontiers
                .read("store/lines", &Since::Frontier(first))
                .expect("reads"),
            Read::Unknown
        );
    }
}
