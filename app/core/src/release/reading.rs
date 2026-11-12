//! The version a chain reads: which releases are done, the `major.minor` that says so, and
//! each release's items.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::{TodoEvent, TodoId, mk_completed};
use prodrome_core::literal::{Datetime, ProdromeError};
use prodrome_core::policy::{Policy, Untrusted};
use prodrome_core::term::{Term, TermF};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Counted, Plan};
use suzatary_record::record;

/// What became of a todo by an instant, joined over its events: completed once is completed,
/// whatever came after (a reopening does not unship a release), and cancelled below completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Lot {
    /// No completion and no cancellation.
    Open,
    /// Cancelled and never completed: a cancelled release moves no version.
    Cancelled,
    /// Completed at least once.
    Completed,
}

/// Where a release stands in the plan, by the releases settled before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    /// At or before the last settled release: shipped, cancelled or passed over.
    Earlier,
    /// The one after the last settled: the release in progress.
    Next,
    /// After that.
    Later,
}

/// Whether an item is named by its release's spec now, or only by an earlier one. A dropped
/// item stays an item, so the page can say it moved out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Named {
    /// The spec in force names it.
    Current,
    /// Only an earlier spec did.
    Dropped,
}

/// The chain read for its version at one instant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    /// The `major.minor` it reads: the last completed release's.
    pub counted: Counted,
    /// Each declared release, in the plan's order.
    pub releases: Vec<ReleaseReading>,
}

/// One release as the chain reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseReading {
    /// Its todo.
    pub todo: TodoId,
    /// The `major.minor` its completion moves to.
    pub version: Counted,
    /// What became of it.
    pub lot: Lot,
    /// Where it stands in the plan.
    pub place: Place,
    /// Every todo its spec has named, in id order.
    pub items: Vec<Item>,
}

/// One item of a release: it shows the release's progress and moves no version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// The todo.
    pub todo: TodoId,
    /// What became of it.
    pub lot: Lot,
    /// Whether the release's spec names it now.
    pub named: Named,
}

/// The events of `nodes` that bind under `policy`: what a reading counts. An agent's write is
/// a claim until confirmed, and moves no version.
pub fn binding<'a>(
    nodes: &'a [record::Node],
    policy: &Untrusted,
) -> impl Iterator<Item = &'a record::TodoEvent> {
    nodes.iter().filter_map(move |node| {
        node.event()
            .filter(|event| policy.standing(node.name(), *event).binds())
    })
}

impl Reading {
    /// The reading of `events` (each binding: [`binding`]) at `at`, for `plan`, with `current`
    /// each todo's spec in force (its candidates, as the fold reads them):
    ///
    /// - an event counts when it is stamped at or before `at`;
    /// - the `major.minor` is the last completed release's, `plan.base()`.0 before any; the
    ///   plan's versions strictly increase, so that is the greatest completed one;
    /// - a release's items are every todo its spec has named (`Ref`, or `RefIn` its own chain),
    ///   in any record or revision, and never a release itself; they move no version.
    ///
    /// A todo's lot is a join over its events, so the reading is a function of the SET (no
    /// order, no clock but `at`) and MONOTONE: more events, or a later `at`, read a
    /// `major.minor` no lower. A reopened release stays shipped, which is what monotone costs.
    #[must_use]
    pub fn of<'a>(
        plan: &Plan,
        events: impl IntoIterator<Item = &'a record::TodoEvent>,
        at: Datetime,
        current: &BTreeMap<TodoId, Vec<Term>>,
    ) -> Reading {
        let (lots, named) = gathered(plan, events, at);
        let lot = |todo: &str| lots.get(todo).copied().unwrap_or(Lot::Open);

        let lots_of: Vec<Lot> = plan
            .releases()
            .iter()
            .map(|r| lot(r.todo.as_str()))
            .collect();
        let counted = plan
            .releases()
            .iter()
            .zip(&lots_of)
            .filter(|(_, lot)| **lot == Lot::Completed)
            .map(|(release, _)| release.version)
            .fold(
                Counted {
                    major: plan.base(),
                    minor: 0,
                },
                Counted::max,
            );
        let next = lots_of
            .iter()
            .rposition(|lot| *lot != Lot::Open)
            .map_or(0, |i| i + 1);
        let place = |i: usize| match i.cmp(&next) {
            std::cmp::Ordering::Less => Place::Earlier,
            std::cmp::Ordering::Equal => Place::Next,
            std::cmp::Ordering::Greater => Place::Later,
        };

        let releases = plan
            .releases()
            .iter()
            .enumerate()
            .map(|(i, release)| {
                let now: BTreeSet<String> = current
                    .get(&release.todo)
                    .into_iter()
                    .flatten()
                    .flat_map(|spec| refs(spec, plan.chain()).collect::<Vec<_>>())
                    .collect();
                let items = named
                    .get(release.todo.as_str())
                    .into_iter()
                    .flatten()
                    .filter_map(|id| TodoId::new(id.clone()).ok())
                    .map(|todo| Item {
                        lot: lot(todo.as_str()),
                        named: if now.contains(todo.as_str()) {
                            Named::Current
                        } else {
                            Named::Dropped
                        },
                        todo,
                    })
                    .collect();
                ReleaseReading {
                    todo: release.todo.clone(),
                    version: release.version,
                    lot: lots_of[i],
                    place: place(i),
                    items,
                }
            })
            .collect();
        Reading { counted, releases }
    }

    /// [`Reading::of`] with `assumed` completed at `at` by `actor` as well: what the chain will
    /// read once those completions are written, as a release is cut before its train lands and
    /// records them.
    ///
    /// # Errors
    /// An `actor` the chain could not hold.
    pub fn assuming<'a>(
        plan: &Plan,
        events: impl IntoIterator<Item = &'a record::TodoEvent>,
        assumed: &[TodoId],
        actor: &str,
        at: Datetime,
        current: &BTreeMap<TodoId, Vec<Term>>,
    ) -> Result<Reading, ProdromeError> {
        let assumed = assumed
            .iter()
            .map(|todo| mk_completed(todo.as_str(), at, actor, ""))
            .collect::<Result<Vec<record::TodoEvent>, _>>()?;
        let mut all: Vec<&record::TodoEvent> = events.into_iter().collect();
        all.extend(assumed.iter());
        Ok(Reading::of(plan, all, at, current))
    }
}

/// Each todo's lot, and every todo each release's spec has named, joined over `events`
/// stamped at or before `at`.
fn gathered<'a>(
    plan: &Plan,
    events: impl IntoIterator<Item = &'a record::TodoEvent>,
    at: Datetime,
) -> (BTreeMap<String, Lot>, BTreeMap<String, BTreeSet<String>>) {
    let releases: BTreeSet<&str> = plan.releases().iter().map(|r| r.todo.as_str()).collect();
    let mut lots: BTreeMap<String, Lot> = BTreeMap::new();
    let mut named: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for event in events {
        if event.at() > at {
            continue;
        }
        let spec = match event {
            TodoEvent::Completed(e) => {
                join(&mut lots, &e.todo, Lot::Completed);
                None
            }
            TodoEvent::Cancelled(e) => {
                join(&mut lots, &e.todo, Lot::Cancelled);
                None
            }
            TodoEvent::SpecRevised(e) => Some(&e.spec),
            TodoEvent::Authored(e) => Some(&e.payload.spec),
            TodoEvent::Created(_) | TodoEvent::Reopened(_) | TodoEvent::Tended(_) => None,
        };
        if let Some(spec) = spec.filter(|_| releases.contains(event.todo().as_str())) {
            named
                .entry(event.todo().as_str().to_owned())
                .or_default()
                .extend(refs(spec, plan.chain()).filter(|id| !releases.contains(id.as_str())));
        }
    }
    (lots, named)
}

/// `todo`'s lot joined with `lot`.
fn join(lots: &mut BTreeMap<String, Lot>, todo: &TodoId, lot: Lot) {
    let held = lots.entry(todo.as_str().to_owned()).or_insert(Lot::Open);
    *held = (*held).max(lot);
}

/// The todos `spec` refers to on `chain`: its unqualified `Ref`s and its `RefIn`s of `chain`.
fn refs<'t>(spec: &'t Term, chain: &'t str) -> impl Iterator<Item = String> + 't {
    spec.cata(|layer: TermF<BTreeSet<String>>| match layer {
        TermF::Ref { store, entity } if store.as_deref().is_none_or(|s| s == chain) => {
            BTreeSet::from([entity])
        }
        other => other.children().into_iter().flatten().cloned().collect(),
    })
    .into_iter()
}
