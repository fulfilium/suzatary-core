//! The memory's history as one value: a two-phase set, the facts remembered and the facts
//! forgotten, each by id; what stands is the first half minus the second, less every fact a
//! consolidation replaced.
//!
//! Ordered by what it holds ([`Held`], inclusion of both halves), and a function of the object
//! set: each fact's half is read off its register ([`crate::secretary::memory_event::LifeRegister`]), which
//! is inflationary, so a larger history holds more and the union of two histories holds the join
//! of what each does. Nothing here reads a fact's words; [`crate::secretary::memory::Memory`]
//! joins them.
//!
//! A consolidation is a remembering whose provenance is its sources' ([`Provenance`]). Law 11
//! (`app/core/tests/memory.rs`, `a_consolidation_replaces_its_sources`) has two clauses:
//!
//! 1. When every source stands, `live(h + C) = live(h) − from + {into}`, and the sources' words
//!    are dead. One box refuses any other consolidation, under the memory's hold.
//! 2. When a source rests on a forgotten fact, which only another replica's forgetting can make,
//!    `live(h + C) = live(h)`: the consolidation reads forgotten, so its words are dead, and it
//!    replaces nothing, so its other sources stand on as they did. A forgotten fact's words never
//!    stand again inside another fact.
//!
//! Forgetting the consolidation itself is the owner's word on all of it: it reads forgotten and
//! its sources stay replaced.
//!
//! A consolidation replaces its sources from the moment it is held, before every source is, so
//! no source ever stands beside a consolidation of it; it stands itself once every source is
//! held. Two rememberings of one number (two replicas counting alike) read as the least-named
//! one; the other's words are named by nothing that stands, so they are dead, and its sources
//! are replaced by nothing.
//!
//! Sources are numbered below their consolidation, so one pass in ascending order reads every
//! fact's provenance, forgetting and sources once.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::{Actor, Hash};
use prodrome_core::fold::sole::{One, Only};
use prodrome_core::fold::{Order, Product as _};
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Everything;
use prodrome_core::registers::{Node, Stamp, fold};

use crate::secretary::memory::{FactId, Kind, Origin};
use crate::secretary::memory_event::{LifeRegister, MemoryEvent, Said};
use crate::secretary::provenance::{Origins, Provenance, Wrote};
use crate::secretary::salted_text::TextName;
use crate::secretary::texts::{Liveness, Naming};

/// The memory's two-phase set.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TwoPhase {
    remembered: BTreeMap<FactId, Kept>,
    /// The words of every remembering that a less-named one of its number outranks.
    outranked: BTreeSet<TextName>,
    forgotten: BTreeMap<FactId, Gone>,
    derived: Derived,
}

/// What the history says of each fact, read once from the two halves.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Derived {
    /// Each fact whose sources are all held, with its provenance.
    provenance: BTreeMap<FactId, Provenance>,
    /// Each fact that reads forgotten, with the forgotten fact it rests on (itself, or the least
    /// source that reads forgotten).
    forgetting: BTreeMap<FactId, FactId>,
    /// Each consolidation's sources, at every depth.
    beneath: BTreeMap<FactId, BTreeSet<FactId>>,
    /// Each fact a consolidation replaced, with those that did, unless the consolidation reads
    /// forgotten through a source of its own.
    replaced: BTreeMap<FactId, BTreeSet<FactId>>,
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

/// A remembering or a consolidation, as its event wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// The object that wrote it.
    pub object: Hash,
    /// When.
    pub at: Datetime,
    /// The name of its words in the text store.
    pub text: TextName,
    /// About whom or what.
    pub kind: Kind,
    /// How it was written.
    pub wrote: Wrote,
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
        let mut remembered = BTreeMap::new();
        let mut outranked = BTreeSet::new();
        let mut forgotten = BTreeMap::new();
        for (&fact, stream) in folded.entities() {
            let registers: One<'_, LifeRegister> =
                prodrome_core::fold::read(stream, None, &Everything);
            if let Some(gone) = registers.reading(Only).iter().find_map(|stamp| gone(stamp)) {
                forgotten.insert(fact, gone);
            }
            let mut written: Vec<Kept> = stream.iter().filter_map(kept).collect();
            written.sort_by(|one, two| one.object.cmp(&two.object));
            let mut written = written.into_iter();
            if let Some(least) = written.next() {
                outranked.extend(written.map(|other| other.text));
                remembered.insert(fact, least);
            }
        }
        Self::over(remembered, outranked, forgotten)
    }

    /// The reading of these halves, every fact's derived once.
    fn over(
        remembered: BTreeMap<FactId, Kept>,
        mut outranked: BTreeSet<TextName>,
        forgotten: BTreeMap<FactId, Gone>,
    ) -> Self {
        let mut derived = Derived::default();
        for (&fact, kept) in &remembered {
            let forgets = |source: FactId, derived: &Derived| {
                if forgotten.contains_key(&source) {
                    Some(source)
                } else {
                    derived.forgetting.get(&source).copied()
                }
            };
            match &kept.wrote {
                Wrote::Said(origin) => {
                    derived
                        .provenance
                        .insert(fact, Provenance::said(origin.clone()));
                }
                Wrote::Merged { from, .. } => {
                    let mut origins: Option<Option<Origins>> = Some(None);
                    let mut beneath = BTreeSet::new();
                    for &source in from {
                        beneath.insert(source);
                        beneath.extend(derived.beneath.get(&source).into_iter().flatten());
                        origins = match (origins, derived.provenance.get(&source)) {
                            (Some(None), Some(theirs)) => Some(Some(theirs.origins.clone())),
                            (Some(Some(ours)), Some(theirs)) => {
                                Some(Some(ours.join(theirs.origins.clone())))
                            }
                            (Some(_) | None, None) | (None, Some(_)) => None,
                        };
                    }
                    if let Some(Some(origins)) = origins {
                        derived.provenance.insert(
                            fact,
                            Provenance {
                                wrote: kept.wrote.clone(),
                                origins,
                            },
                        );
                    }
                    derived.beneath.insert(fact, beneath);
                }
            }
            let forgetting = match &kept.wrote {
                _ if forgotten.contains_key(&fact) => Some(fact),
                Wrote::Said(_) => None,
                Wrote::Merged { from, .. } => {
                    from.iter().find_map(|&source| forgets(source, &derived))
                }
            };
            if let Some(on) = forgetting {
                derived.forgetting.insert(fact, on);
            }
        }
        for (&into, kept) in &remembered {
            // A consolidation the owner forgot still replaced its sources; one that reads
            // forgotten through a source replaced nothing that stands.
            if let Wrote::Merged { from, .. } = &kept.wrote
                && derived.forgetting.get(&into).is_none_or(|on| *on == into)
            {
                for &source in from {
                    derived.replaced.entry(source).or_default().insert(into);
                }
            }
        }
        // A text both outranked and kept stays named by the one kept.
        outranked.retain(|text| remembered.values().all(|kept| &kept.text != text));
        Self {
            remembered,
            outranked,
            forgotten,
            derived,
        }
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

    /// The facts that stand: remembered or consolidated, every source held, nothing it rests on
    /// forgotten, and replaced by no consolidation, except one that reads forgotten through a
    /// source of its own.
    pub fn live(&self) -> impl Iterator<Item = (FactId, &Kept)> {
        self.remembered
            .iter()
            .filter(|(fact, _)| {
                self.derived.provenance.contains_key(fact)
                    && !self.derived.forgetting.contains_key(fact)
                    && !self.derived.replaced.contains_key(fact)
            })
            .map(|(&fact, kept)| (fact, kept))
    }

    /// Where fact `fact` comes from: `None` for a fact not remembered, or consolidated from a fact
    /// the history does not hold yet.
    #[must_use]
    pub fn provenance(&self, fact: FactId) -> Option<&Provenance> {
        self.derived.provenance.get(&fact)
    }

    /// The forgetting fact `fact` reads forgotten by: its own, or for a consolidation, the
    /// forgetting of the least source that reads forgotten.
    #[must_use]
    pub fn forgetting(&self, fact: FactId) -> Option<&Gone> {
        self.forgotten.get(&fact).or_else(|| {
            self.derived
                .forgetting
                .get(&fact)
                .and_then(|on| self.forgotten.get(on))
        })
    }

    /// Every fact a consolidation replaced, with the facts that replaced it, leaving out a
    /// consolidation that reads forgotten through a source of its own.
    #[must_use]
    pub fn replaced(&self) -> &BTreeMap<FactId, BTreeSet<FactId>> {
        &self.derived.replaced
    }

    /// Every fact whose words a forgetting of `fact` takes: the fact, every source it rests on,
    /// and every consolidation resting on it, each with the round whose call wrote it.
    #[must_use]
    pub fn forgotten_with(&self, fact: FactId) -> BTreeMap<FactId, &Origin> {
        let above = self
            .derived
            .beneath
            .iter()
            .filter(|(_, beneath)| beneath.contains(&fact))
            .map(|(&into, _)| into);
        std::iter::once(fact)
            .chain(
                self.derived
                    .beneath
                    .get(&fact)
                    .into_iter()
                    .flatten()
                    .copied(),
            )
            .chain(above)
            .filter_map(|taken| Some((taken, self.remembered.get(&taken)?.wrote.round())))
            .collect()
    }

    /// The texts the history names: each fact's, live while the fact stands or waits for a
    /// source it names, and past once it is forgotten or replaced, or outranked by a less-named
    /// remembering of its number.
    #[must_use]
    pub fn naming(&self) -> Naming<crate::secretary::memory::Fact> {
        self.remembered
            .iter()
            .map(|(fact, kept)| {
                let liveness = if self.derived.forgetting.contains_key(fact)
                    || self.derived.replaced.contains_key(fact)
                {
                    Liveness::Past
                } else {
                    Liveness::Live
                };
                (kept.text.clone(), liveness)
            })
            .chain(
                self.outranked
                    .iter()
                    .map(|text| (text.clone(), Liveness::Past)),
            )
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

    /// The least set above both: each half's union, read again. Where both hold a remembering of
    /// one fact, the least-named is kept and the other's words are outranked, as a reading of
    /// both histories' objects together keeps them.
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        let (mut remembered, mut outranked, mut forgotten) =
            (self.remembered, self.outranked, self.forgotten);
        outranked.extend(other.outranked);
        for (fact, kept) in other.remembered {
            match remembered.get_mut(&fact) {
                Some(ours) if ours.object == kept.object => {}
                Some(ours) if kept.object < ours.object => {
                    outranked.insert(std::mem::replace(ours, kept).text);
                }
                Some(_) => {
                    outranked.insert(kept.text);
                }
                None => {
                    remembered.insert(fact, kept);
                }
            }
        }
        for (fact, gone) in other.forgotten {
            forgotten.entry(fact).or_insert(gone);
        }
        Self::over(remembered, outranked, forgotten)
    }
}

/// Inclusion, half by half: a history read later holds every fact read earlier in each half.
impl Order for Held {
    fn le(&self, other: &Self) -> bool {
        self.remembered.is_subset(&other.remembered) && self.forgotten.is_subset(&other.forgotten)
    }
}

fn kept(stamp: &Stamp<MemoryEvent>) -> Option<Kept> {
    let at = prodrome_core::schema::Schema::at(&*stamp.event);
    let object = stamp.name.clone();
    match stamp.event.body() {
        Said::Remembered { text, kind, origin } => Some(Kept {
            object,
            at,
            text: text.clone(),
            kind: *kind,
            wrote: Wrote::Said(origin.clone()),
        }),
        Said::Consolidated {
            from,
            text,
            kind,
            round,
        } => Some(Kept {
            object,
            at,
            text: text.clone(),
            kind: *kind,
            wrote: Wrote::Merged {
                from: from.clone(),
                round: round.clone(),
            },
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
        Said::Remembered { .. } | Said::Consolidated { .. } => None,
    }
}
