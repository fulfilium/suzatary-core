//! The one list: every open task of every store the set holds, each row with its store and its
//! package, ordered by its reading under the waits ([`crate::waits`]), most urgent first. One
//! store's list is this list restricted to that store's rows.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;
use prodrome_core::policy::{Policy, Standing};
use prodrome_core::schema::Schema;
use prodrome_core::{fold, registers};
use serde::Serialize;
use suzatary_base::chains::ChainName;
use suzatary_base::escape::string_text;
use suzatary_base::reference::{GenesisPrefix, GlobalTodo, Reference, TodoRef, mentions};
use suzatary_record::decoration::records_of;
use suzatary_record::record::{Authored, Entry};
use suzatary_signatures::chain_set::{ChainSet, NotAMember, home};

use crate::wait::{self, Held, Kept, Missing, Wait};
use crate::waits::{self, Reading};
use suzatary_todo::shown::{OpenRow, open, open_row, percent};

/// One open task of the one list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListRow {
    /// The store it lives in, by the name this deployment gives it.
    pub chain: ChainName,
    /// Its name in every store and on every deployment, its prodrome's whole genesis.
    #[serde(serialize_with = "spelled")]
    pub name: GlobalTodo,
    #[serde(flatten)]
    pub row: OpenRow,
    pub package: Package,
    pub reading: Urgency,
}

/// A row's reading, as the list prints it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type")]
pub enum Urgency {
    /// Its own price orders it.
    Own,
    /// A task that waits on it is more urgent, and its price orders this row.
    Raised { percent: String, by: TodoRef },
}

/// The package a task lives in: its record's category, checked where its store's units are
/// known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "name")]
pub enum Package {
    /// One of its store's units.
    Unit(String),
    /// Its store names its units, and none is this.
    Unlisted(String),
    /// Its store names no units here, so the name is as written.
    Unchecked(String),
}

/// How the list reads one store.
#[derive(Debug, Clone, Copy)]
pub struct Store<'a> {
    pub chain: &'a ChainName,
    pub kept: Kept,
    pub units: Units<'a>,
}

/// A wait the list could not follow: a task's reference to a todo that moves nothing here.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Unresolved {
    pub waiter: TodoRef,
    pub reference: Reference,
    pub missing: Missing,
}

/// The one list, every store that did not read and why, and every wait it could not follow.
#[derive(Debug, Clone)]
pub struct Listed {
    pub rows: Vec<ListRow>,
    pub failed: Vec<NotAMember>,
    pub unresolved: Vec<Unresolved>,
}

/// The units a store's categories are checked against.
#[derive(Debug, Clone, Copy)]
pub enum Units<'a> {
    Listed(&'a BTreeSet<String>),
    Unknown,
}

/// The one list of `stores`, in their order, read through `set`. A store absent from the set
/// reads as no rows, and a wait into it moves nothing and is said in `unresolved`.
#[must_use]
pub fn listed(set: &ChainSet<'_>, stores: &[Store<'_>]) -> Listed {
    let mut failed = Vec::new();
    let mut read_stores = Vec::new();
    let mut holding = Held::default();
    for (place, store) in stores.iter().enumerate() {
        let read = set.held(store.chain).and_then(|chain| {
            Ok((
                records_of(chain.nodes),
                set.entries(store.chain)?,
                registers::fold(chain.nodes),
                chain.untrusted,
                // A store of several geneses does not read, so `entries` refused it already.
                home(chain.dag).unwrap_or(None),
            ))
        });
        match read {
            Ok((records, entries, folded, untrusted, home)) => {
                if let Some(home) = &home {
                    holding = holding.with(store.chain.clone(), home.clone(), store.kept);
                }
                read_stores.push((place, store, records, entries, folded, untrusted, home));
            }
            Err(why) => failed.push(why),
        }
    }
    let now = set.instant();
    let open_tasks: Vec<OpenTask<'_>> = read_stores
        .iter()
        .flat_map(
            |(place, store, records, entries, folded, untrusted, home)| {
                open(entries, records)
                    .into_iter()
                    .filter_map(move |(entry, record)| {
                        // A row is a record's, so its store holds a prodrome: a genesis or a root.
                        let prodrome = entry.genesis.clone().or_else(|| home.clone())?;
                        let confirmed = folded
                            .prodromes()
                            .get(&entry.genesis)
                            .and_then(|prodrome| prodrome.get(&entry.key))
                            .map(|stream| {
                                fold::read(stream, Some(now), &Believing(*untrusted)).records()
                            })
                            .unwrap_or_default();
                        Some(OpenTask {
                            place: *place,
                            store,
                            prodrome,
                            entry,
                            record,
                            confirmed,
                        })
                    })
            },
        )
        .collect();
    let refs: Vec<TodoRef> = open_tasks
        .iter()
        .map(|open| TodoRef {
            chain: open.store.chain.clone(),
            id: open.entry.key.clone(),
        })
        .collect();
    let own: Vec<Option<f64>> = open_tasks
        .iter()
        .map(|open| open.entry.value().ok().flatten())
        .collect();
    let (edges, unresolved) = followed(&open_tasks, &refs, &holding);
    let readings = waits::readings(&own, &edges);
    Listed {
        rows: ordered(open_tasks, readings, &refs),
        failed,
        unresolved,
    }
}

/// Each open task's row, ordered by its reading, most urgent first.
fn ordered(
    open_tasks: Vec<OpenTask<'_>>,
    readings: Vec<Reading>,
    refs: &[TodoRef],
) -> Vec<ListRow> {
    let mut rows: Vec<(Option<f64>, usize, &Entry, ListRow)> = open_tasks
        .into_iter()
        .zip(readings)
        .map(|(open, reading)| {
            let OpenTask {
                place,
                store,
                prodrome,
                entry,
                record,
                ..
            } = open;
            let row = open_row(entry, record);
            (
                reading.value(entry.value().ok().flatten()),
                place,
                entry,
                ListRow {
                    chain: store.chain.clone(),
                    name: GlobalTodo {
                        genesis: GenesisPrefix::whole(&prodrome),
                        id: entry.key.clone(),
                    },
                    package: package(store.units, &row.category),
                    row,
                    reading: urgency(reading, refs),
                },
            )
        })
        .collect();
    // Restricted to one store whose readings are all its own, this order is `view::list_order`.
    rows.sort_by(|(x, a, left, _), (y, b, right, _)| {
        match (x, y) {
            (Some(x), Some(y)) => x.total_cmp(y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
        .then_with(|| a.cmp(b))
        .then_with(|| (&left.genesis, &left.key).cmp(&(&right.genesis, &right.key)))
    });
    rows.into_iter().map(|(.., row)| row).collect()
}

/// One open task of a store that read, with the records its store's policy confirms.
struct OpenTask<'a> {
    place: usize,
    store: &'a Store<'a>,
    /// Its prodrome: its own genesis, or its store's least root where it is a legacy one.
    prodrome: Hash,
    entry: &'a Entry,
    record: &'a Authored,
    confirmed: Vec<&'a Authored>,
}

/// The waits among `open`, as `(waiter, blocker)` indices, and every one that moves nothing.
/// A wait is read from each record the store's policy confirms, whatever is claimed over it,
/// and from every candidate of a conflict between them: a claim's `waiting_on` is shown on its
/// row and moves no other. A wait on a task held and not open moves nothing either.
fn followed(
    open: &[OpenTask<'_>],
    refs: &[TodoRef],
    holding: &Held,
) -> (Vec<(usize, usize)>, Vec<Unresolved>) {
    let index: BTreeMap<&TodoRef, usize> = refs.iter().enumerate().map(|(i, r)| (r, i)).collect();
    let mut edges = Vec::new();
    let mut unresolved = Vec::new();
    for (waiter, task) in open.iter().enumerate() {
        for mention in task
            .confirmed
            .iter()
            .flat_map(|record| mentions(&string_text(record.payload.waiting_on.as_str())))
        {
            match wait::blocker(task.store.kept.audience(), &mention, holding) {
                Wait::On(blocker) => edges.extend(index.get(&blocker).map(|b| (waiter, *b))),
                Wait::Label => {}
                Wait::Missing(missing) => unresolved.push(Unresolved {
                    waiter: refs[waiter].clone(),
                    reference: mention,
                    missing,
                }),
            }
        }
    }
    unresolved.sort();
    unresolved.dedup();
    (edges, unresolved)
}

fn spelled<S: serde::Serializer>(name: &GlobalTodo, to: S) -> Result<S::Ok, S::Error> {
    to.collect_str(name)
}

fn package(units: Units<'_>, category: &str) -> Package {
    match units {
        Units::Listed(units) if units.contains(category) => Package::Unit(category.to_owned()),
        Units::Listed(_) => Package::Unlisted(category.to_owned()),
        Units::Unknown => Package::Unchecked(category.to_owned()),
    }
}

fn urgency(reading: Reading, refs: &[TodoRef]) -> Urgency {
    match reading {
        Reading::Own => Urgency::Own,
        Reading::Raised { value, by } => Urgency::Raised {
            percent: percent(value),
            by: refs[by].clone(),
        },
    }
}

/// A store's policy with every write it does not confirm a claim, content records included:
/// what the store confirms, and so the only writes a wait is read from.
struct Believing<'a, P>(&'a P);

impl<E: Schema, P: Policy<E>> Policy<E> for Believing<'_, P> {
    fn standing(&self, object: &Hash, event: &E) -> Standing {
        if self.0.confirms(object, event) {
            Standing::Binds
        } else {
            Standing::Claims
        }
    }
}
