//! The memory's history as one value: a two-phase set, the facts remembered and the facts
//! forgotten, each by id; what stands is the first half minus the second.
//!
//! Ordered by what it holds ([`Held`], inclusion of both halves), and a function of the object
//! set: each fact's half is read off its register ([`crate::secretary::memory_event::LifeRegister`]), which
//! is inflationary, so a larger history holds more and the union of two histories holds the join
//! of what each does. Nothing here reads a fact's words; [`crate::secretary::memory::Memory`]
//! joins them.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Actor;
use prodrome_core::fold::sole::{One, Only};
use prodrome_core::fold::{Order, Product as _};
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Everything;
use prodrome_core::registers::{Node, Stamp, fold};

use crate::secretary::memory::{FactId, Kind, Origin};
use crate::secretary::memory_event::{LifeRegister, MemoryEvent, Said};
use crate::secretary::salted_text::TextName;
use crate::secretary::texts::{Liveness, Naming};

/// The memory's two-phase set.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TwoPhase {
    remembered: BTreeMap<FactId, Kept>,
    forgotten: BTreeMap<FactId, Gone>,
}

/// What a two-phase set holds, as its order sees it: the ids of each half. The records beside the
/// ids are not ordered, so two histories that numbered one fact twice hold the same ids and still
/// read apart.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Held {
    /// The facts remembered, standing or not.
    pub remembered: BTreeSet<FactId>,
    /// The facts forgotten.
    pub forgotten: BTreeSet<FactId>,
}

/// A remembering, as its event wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// When.
    pub at: Datetime,
    /// The name of its words in the text store.
    pub text: TextName,
    /// About whom or what.
    pub kind: Kind,
    /// Who said it.
    pub said: Origin,
}

/// A forgetting, as its event wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gone {
    /// When.
    pub at: Datetime,
    /// Who.
    pub by: Actor,
    /// The memory's version it made.
    pub version: u64,
}

impl TwoPhase {
    /// The history's reading: a fact is forgotten where its register reads a forgetting, and
    /// remembered where its history holds a remembering, the least-named one should two disagree.
    #[must_use]
    pub fn of(nodes: &[Node<MemoryEvent>]) -> Self {
        let folded = fold(nodes);
        let mut read = Self::default();
        for (&fact, stream) in folded.entities() {
            let registers: One<'_, LifeRegister> =
                prodrome_core::fold::read(stream, None, &Everything);
            if let Some(gone) = registers.reading(Only).iter().find_map(|stamp| gone(stamp)) {
                read.forgotten.insert(fact, gone);
            }
            if let Some(kept) = stream
                .iter()
                .filter_map(|stamp| kept(stamp).map(|kept| (&stamp.name, kept)))
                .min_by(|one, two| one.0.cmp(two.0))
                .map(|(_, kept)| kept)
            {
                read.remembered.insert(fact, kept);
            }
        }
        read
    }

    /// Every remembering, by fact, standing or not.
    #[must_use]
    pub fn remembered(&self) -> &BTreeMap<FactId, Kept> {
        &self.remembered
    }

    /// Every forgetting, by fact.
    #[must_use]
    pub fn forgotten(&self) -> &BTreeMap<FactId, Gone> {
        &self.forgotten
    }

    /// The facts that stand: remembered and not forgotten.
    pub fn live(&self) -> impl Iterator<Item = (FactId, &Kept)> {
        self.remembered
            .iter()
            .filter(|(fact, _)| !self.forgotten.contains_key(fact))
            .map(|(&fact, kept)| (fact, kept))
    }

    /// The texts the history names: each fact's, live while the fact stands.
    #[must_use]
    pub fn naming(&self) -> Naming<crate::secretary::memory::Fact> {
        self.remembered
            .iter()
            .map(|(fact, kept)| {
                let liveness = if self.forgotten.contains_key(fact) {
                    Liveness::Past
                } else {
                    Liveness::Live
                };
                (kept.text.clone(), liveness)
            })
            .collect()
    }

    /// The highest version any event made; 0 for an empty memory.
    #[must_use]
    pub fn version(&self) -> u64 {
        let remembered = self.remembered.keys().map(|fact| fact.0);
        let forgotten = self
            .forgotten
            .iter()
            .map(|(fact, gone)| gone.version.max(fact.0));
        remembered.chain(forgotten).max().unwrap_or(0)
    }

    /// What it holds, as its order sees it.
    #[must_use]
    pub fn held(&self) -> Held {
        Held {
            remembered: self.remembered.keys().copied().collect(),
            forgotten: self.forgotten.keys().copied().collect(),
        }
    }

    /// The least set above both: each half's union. Where both hold a record of one fact, the
    /// first's is kept: the two agree unless two writers numbered one fact twice, which the order,
    /// comparing ids alone, does not see either.
    #[must_use]
    pub fn join(mut self, other: Self) -> Self {
        for (fact, kept) in other.remembered {
            self.remembered.entry(fact).or_insert(kept);
        }
        for (fact, gone) in other.forgotten {
            self.forgotten.entry(fact).or_insert(gone);
        }
        self
    }
}

/// Inclusion, half by half: a history read later holds every fact read earlier in each half.
impl Order for Held {
    fn le(&self, other: &Self) -> bool {
        self.remembered.is_subset(&other.remembered) && self.forgotten.is_subset(&other.forgotten)
    }
}

fn kept(stamp: &Stamp<MemoryEvent>) -> Option<Kept> {
    match stamp.event.body() {
        Said::Remembered { text, kind, origin } => Some(Kept {
            at: prodrome_core::schema::Schema::at(&*stamp.event),
            text: text.clone(),
            kind: *kind,
            said: origin.clone(),
        }),
        Said::Forgotten { .. } => None,
    }
}

fn gone(stamp: &Stamp<MemoryEvent>) -> Option<Gone> {
    match stamp.event.body() {
        Said::Forgotten { version } => Some(Gone {
            at: prodrome_core::schema::Schema::at(&*stamp.event),
            by: prodrome_core::schema::Schema::actor(&*stamp.event).clone(),
            version: *version,
        }),
        Said::Remembered { .. } => None,
    }
}
