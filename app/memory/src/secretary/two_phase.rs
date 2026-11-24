//! The memory's history as a two-phase set: what stands is the remembered minus the forgotten, less
//! every fact a consolidation replaced.
//!
//! It is a function of the object set, ordered by [`Held`]; nothing here reads a fact's words.
//!
//! Law 11 (`app/stores/tests/memory.rs`, `a_consolidation_replaces_its_sources`) has two clauses:
//!
//! 1. When every source stands, `live(h + C) = live(h) − from + {into}`, and the sources' words
//!    are dead. One box refuses any other consolidation, under the memory's hold.
//! 2. When a source rests on a forgotten fact, which only another replica's forgetting can make,
//!    `live(h + C) = live(h)` and the consolidation's words are dead.
//!    A forgotten fact's words never stand again inside another fact.
//!
//! A consolidation replaces its sources from the moment it is held, so no source stands beside it.
//! Two rememberings of one number read as the least-named one, and the other's words are dead.
//!
//! Sources are numbered below their consolidation, so one pass in ascending order reads every fact
//! once.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_dag::registers::{Node, Stamp, fold};
use prodrome_data::event::{Actor, Hash};
use prodrome_data::fold::sole::{One, Only};
use prodrome_data::fold::{Order, Product as _};
use prodrome_data::literal::Datetime;
use prodrome_data::policy::Everything;

use crate::secretary::memory::{FactId, Kind, Origin};
use crate::secretary::memory_event::{LifeRegister, MemoryEvent, Said};
use crate::secretary::provenance::{Origins, Provenance, Wrote};
use crate::secretary::salted_text::TextName;
use crate::secretary::texts::{Liveness, Naming};
use prodrome_data::schema::Schema;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TwoPhase {
    remembered: BTreeMap<FactId, Kept>,
    /// The words of every remembering that a less-named one of its number outranks.
    outranked: BTreeSet<TextName>,
    forgotten: BTreeMap<FactId, Gone>,
    derived: Derived,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Derived {
    /// Each fact whose sources are all held.
    provenance: BTreeMap<FactId, Provenance>,
    /// Each fact that reads forgotten, with the forgotten fact it rests on.
    forgetting: BTreeMap<FactId, FactId>,
    /// Each consolidation's sources, at every depth.
    beneath: BTreeMap<FactId, BTreeSet<FactId>>,
    /// Leaves out a consolidation that reads forgotten through a source of its own.
    replaced: BTreeMap<FactId, BTreeSet<FactId>>,
}

/// The ids of each half, as the order sees them; two histories that numbered one fact twice hold
/// the same ids.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Held {
    /// Standing or not.
    pub remembered: BTreeSet<FactId>,
    pub forgotten: BTreeSet<FactId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub object: Hash,
    pub at: Datetime,
    pub text: TextName,
    pub kind: Kind,
    pub wrote: Wrote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gone {
    pub at: Datetime,
    pub by: Actor,
    pub version: u64,
}

impl TwoPhase {
    /// Two rememberings of one fact read as the least-named one.
    #[must_use]
    pub fn of(nodes: &[Node<MemoryEvent>]) -> Self {
        let folded = fold(nodes);
        let mut remembered = BTreeMap::new();
        let mut outranked = BTreeSet::new();
        let mut forgotten = BTreeMap::new();
        for (&fact, stream) in folded.entities() {
            let registers: One<'_, LifeRegister> =
                prodrome_dag::fold::read(stream, None, &Everything);
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
            // A forgotten consolidation still replaced its sources; one forgotten through a source
            // replaced nothing.
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

    #[must_use]
    pub fn remembered(&self) -> &BTreeMap<FactId, Kept> {
        &self.remembered
    }

    #[must_use]
    pub fn forgotten(&self) -> &BTreeMap<FactId, Gone> {
        &self.forgotten
    }

    /// The facts that stand: every source held, nothing beneath forgotten, and not replaced.
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

    /// `None` also for a fact consolidated from a fact the history does not hold yet.
    #[must_use]
    pub fn provenance(&self, fact: FactId) -> Option<&Provenance> {
        self.derived.provenance.get(&fact)
    }

    /// Its own forgetting, or for a consolidation the least forgotten source's.
    #[must_use]
    pub fn forgetting(&self, fact: FactId) -> Option<&Gone> {
        self.forgotten.get(&fact).or_else(|| {
            self.derived
                .forgetting
                .get(&fact)
                .and_then(|on| self.forgotten.get(on))
        })
    }

    /// Leaves out a consolidation that reads forgotten through a source of its own.
    #[must_use]
    pub fn replaced(&self) -> &BTreeMap<FactId, BTreeSet<FactId>> {
        &self.derived.replaced
    }

    /// Every fact whose words a forgetting of `fact` takes: its sources and every consolidation on
    /// it.
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

    /// A text is past once its fact is forgotten, replaced or outranked.
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

    /// 0 for an empty memory.
    #[must_use]
    pub fn version(&self) -> u64 {
        let remembered = self.remembered.keys().map(|fact| fact.0);
        let forgotten = self
            .forgotten
            .iter()
            .map(|(fact, gone)| gone.version.max(fact.0));
        remembered.chain(forgotten).max().unwrap_or(0)
    }

    #[must_use]
    pub fn held(&self) -> Held {
        Held {
            remembered: self.remembered.keys().copied().collect(),
            forgotten: self.forgotten.keys().copied().collect(),
        }
    }

    /// Where both hold a remembering of one fact, the least-named is kept, as reading both object
    /// sets together does.
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

/// Inclusion, half by half.
impl Order for Held {
    fn le(&self, other: &Self) -> bool {
        self.remembered.is_subset(&other.remembered) && self.forgotten.is_subset(&other.forgotten)
    }
}

fn kept(stamp: &Stamp<MemoryEvent>) -> Option<Kept> {
    let at = Schema::at(&*stamp.event);
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
            at: Schema::at(&*stamp.event),
            by: Schema::actor(&*stamp.event).clone(),
            version: *version,
        }),
        Said::Remembered { .. } | Said::Consolidated { .. } => None,
    }
}
