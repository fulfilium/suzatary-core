//! A store's rows as its pull moves them: the landed row its readings read, and the pieces of a
//! pull beside it, which no reading reads until they land.

use std::borrow::Cow;
use std::collections::BTreeSet;

use crate::replica::held::{Held, Node, Said};
use crate::replica::row::{Keys, Row};

/// The landed row and the pending pieces. Every move is a [`Delta`] applied to them, so a
/// replica is the deltas it took, applied in order to [`Pulling::default`].
#[derive(Debug, Clone, PartialEq)]
pub struct Pulling<N, K> {
    landed: Row<N, K>,
    pending: Pending<N, K>,
}

/// The pieces of the pull since the last landing. `landings` counts the landings before them, so
/// a landing is a join: it moves the count on with nothing held, which every piece of the count
/// before stands under.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending<N, K> {
    landings: u64,
    row: Row<N, K>,
}

/// One move of a [`Pulling`]. Every kind but [`Delta::Forgot`] is a join, so those apply in any
/// order and any number of times to the same end; a purge of keys is the one step down, and
/// applies in the order it was taken.
#[derive(Debug, Clone, PartialEq)]
pub enum Delta<N, K: Keys> {
    /// A piece kept beside the landed row, asked after `landings` landings.
    Piece {
        /// The landings before it.
        landings: u64,
        /// What it brought, standing where the replica asked from stood.
        row: Row<N, K>,
    },
    /// The pending pieces `row` landed, the `landings`th landing.
    Landed {
        /// The landings, this one included.
        landings: u64,
        /// What landed.
        row: Row<N, K>,
    },
    /// Every row reset to nothing held, in generation `generation`: what a later epoch of the
    /// box, or a row that does not read, starts again from.
    StartedOver {
        /// The generation started.
        generation: u64,
    },
    /// The keys a purge let go, from every row.
    Forgot(BTreeSet<K::Name>),
}

impl<N: Node, K: Keys> Default for Pulling<N, K> {
    fn default() -> Self {
        Pulling {
            landed: Row::empty(0),
            pending: Pending {
                landings: 0,
                row: Row::empty(0),
            },
        }
    }
}

impl<N: Node, K: Keys> Pending<N, K> {
    /// The join: the later generation, then the more landings, kept whole; within both, the rows'.
    fn join(self, other: &Self) -> Self {
        let mine = (self.row.generation(), self.landings);
        let theirs = (other.row.generation(), other.landings);
        match mine.cmp(&theirs) {
            std::cmp::Ordering::Less => other.clone(),
            std::cmp::Ordering::Greater => self,
            std::cmp::Ordering::Equal => Pending {
                landings: self.landings,
                row: self.row.join(&other.row),
            },
        }
    }
}

impl<N: Node, K: Keys> Pulling<N, K> {
    /// `landed`, with the pieces `pending` kept after `landings` landings.
    #[must_use]
    pub fn of(landed: Row<N, K>, landings: u64, pending: Row<N, K>) -> Self {
        Pulling {
            landed,
            pending: Pending {
                landings,
                row: pending,
            },
        }
    }

    /// The row the readings read.
    #[must_use]
    pub fn landed(&self) -> &Row<N, K> {
        &self.landed
    }

    /// The pieces kept and not landed.
    #[must_use]
    pub fn pending(&self) -> &Row<N, K> {
        &self.pending.row
    }

    /// How many landings came before the pending pieces.
    #[must_use]
    pub fn landings(&self) -> u64 {
        self.pending.landings
    }

    /// What is held, landed and not: what a pull asks from.
    #[must_use]
    pub fn holding(&self) -> Cow<'_, Row<N, K>> {
        let pending = &self.pending.row;
        if pending.is_empty() && pending.generation() <= self.landed.generation() {
            Cow::Borrowed(&self.landed)
        } else {
            Cow::Owned(self.landed.clone().join(pending))
        }
    }

    /// Whether the box answered with a later epoch than a word either row heard: its store no
    /// longer extends what it served, so what is held may name nodes it no longer holds. Each row
    /// is asked, not their join, which would hide a landed row under a piece of a later epoch.
    #[must_use]
    pub fn overtaken(&self, epoch: u64) -> bool {
        [&self.landed, &self.pending.row]
            .iter()
            .any(|row| row.replica().spoken().is_some_and(|w| w.epoch < epoch))
    }

    /// Whether the box answered with an earlier epoch than the word held: its own count went
    /// back, which only a lost state directory does.
    #[must_use]
    pub fn went_back(&self, epoch: u64) -> bool {
        self.holding()
            .replica()
            .spoken()
            .is_some_and(|w| w.epoch > epoch)
    }

    /// The piece the box answered, `said` heard at `fetched_at` with the keys `keys`, kept beside
    /// the landed row. It stands where what is held stood, so the frontier it names is read
    /// against the one asked from, and a reset taken meanwhile keeps it out.
    pub fn piece(&mut self, said: Said<N>, keys: K, fetched_at: u64) -> Delta<N, K> {
        let asked = self.holding();
        let replica = Held::standing(asked.generation(), asked.replica().frontier().clone())
            .pulled(said, fetched_at);
        self.took(Delta::Piece {
            landings: self.pending.landings,
            row: Row::of(replica, keys),
        })
    }

    /// The pending pieces landed: joined into the row the readings read.
    pub fn land(&mut self) -> Delta<N, K> {
        self.took(Delta::Landed {
            landings: self.pending.landings.saturating_add(1),
            row: self.pending.row.clone(),
        })
    }

    /// Every row reset, in the generation after both.
    pub fn start_over(&mut self) -> Delta<N, K> {
        let generation = self
            .landed
            .generation()
            .max(self.pending.row.generation())
            .saturating_add(1);
        self.took(Delta::StartedOver { generation })
    }

    /// The keys `gone` let go from every row.
    pub fn forget(&mut self, gone: BTreeSet<K::Name>) -> Delta<N, K> {
        self.took(Delta::Forgot(gone))
    }

    fn took(&mut self, delta: Delta<N, K>) -> Delta<N, K> {
        self.apply(&delta);
        delta
    }

    /// `delta` taken, this replica's own or another's.
    pub fn apply(&mut self, delta: &Delta<N, K>) {
        let landed = std::mem::replace(&mut self.landed, Row::empty(0));
        let pending = std::mem::replace(
            &mut self.pending,
            Pending {
                landings: 0,
                row: Row::empty(0),
            },
        );
        let (landed, pending) = match delta {
            Delta::Piece { landings, row } => (
                landed,
                pending.join(&Pending {
                    landings: *landings,
                    row: row.clone(),
                }),
            ),
            Delta::Landed { landings, row } => (
                landed.join(row),
                pending.join(&Pending {
                    landings: *landings,
                    row: Row::empty(row.generation()),
                }),
            ),
            Delta::StartedOver { generation } => {
                let empty = Row::empty(*generation);
                (
                    landed.join(&empty),
                    pending.join(&Pending {
                        landings: 0,
                        row: empty,
                    }),
                )
            }
            Delta::Forgot(gone) => (
                landed.without(gone),
                Pending {
                    landings: pending.landings,
                    row: pending.row.without(gone),
                },
            ),
        };
        self.landed = landed;
        self.pending = pending;
    }
}
