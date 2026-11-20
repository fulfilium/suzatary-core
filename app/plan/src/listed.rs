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
use suzatary_base::reference::{Reference, TodoRef, mentions};
use suzatary_record::decoration::records_of;
use suzatary_record::record::{Authored, Entry};
use suzatary_signatures::chain_set::{ChainSet, NotAMember};

use crate::waits::{self, Reading};
use suzatary_todo::shown::{OpenRow, open, open_row, percent};

/// One open task of the one list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListRow {
    /// The store it lives in, by the name this deployment gives it.
    pub chain: ChainName,
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
    pub waits: Waits,
    pub units: Units<'a>,
}

/// Which `todo:<chain>/<id>` in a store's `waiting_on` is a wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waits {
    /// A private store of this deployment: its chain names are this deployment's, so each such
    /// reference to a store of the list is a wait.
    ByLocalName,
    /// A public or followed store: a local chain name there means nothing, so `waiting_on` is a
    /// label only.
    None,
}

/// The units a store's categories are checked against.
#[derive(Debug, Clone, Copy)]
pub enum Units<'a> {
    Listed(&'a BTreeSet<String>),
    Unknown,
}

/// The one list of `stores`, in their order, read through `set`, and every store that did not
/// read, with why. A store absent from the set reads as no rows, and a wait into it moves
/// nothing.
#[must_use]
pub fn listed(set: &ChainSet<'_>, stores: &[Store<'_>]) -> (Vec<ListRow>, Vec<NotAMember>) {
    let mut failed = Vec::new();
    let mut held = Vec::new();
    for (place, store) in stores.iter().enumerate() {
        let read = set.held(store.chain).and_then(|chain| {
            Ok((
                records_of(chain.nodes),
                set.entries(store.chain)?,
                registers::fold(chain.nodes),
                chain.untrusted,
            ))
        });
        match read {
            Ok((records, entries, folded, untrusted)) => {
                held.push((place, store, records, entries, folded, untrusted));
            }
            Err(why) => failed.push(why),
        }
    }
    let now = set.instant();
    let open_tasks: Vec<(usize, &Store<'_>, &Entry, &Authored, Vec<&Authored>)> = held
        .iter()
        .flat_map(|(place, store, records, entries, folded, untrusted)| {
            open(entries, records)
                .into_iter()
                .map(move |(entry, record)| {
                    let confirmed = folded
                        .prodromes()
                        .get(&entry.genesis)
                        .and_then(|prodrome| prodrome.get(&entry.key))
                        .map(|stream| {
                            fold::read(stream, Some(now), &Believing(*untrusted)).records()
                        })
                        .unwrap_or_default();
                    (*place, *store, entry, record, confirmed)
                })
        })
        .collect();
    let refs: Vec<TodoRef> = open_tasks
        .iter()
        .map(|(_, store, entry, ..)| TodoRef {
            chain: store.chain.clone(),
            id: entry.key.clone(),
        })
        .collect();
    let index: BTreeMap<&TodoRef, usize> = refs.iter().enumerate().map(|(i, r)| (r, i)).collect();
    let own: Vec<Option<f64>> = open_tasks
        .iter()
        .map(|(_, _, entry, ..)| entry.value().ok().flatten())
        .collect();
    // A wait is read from each record the store's policy confirms, whatever is claimed over it,
    // and from every candidate of a conflict between them: a claim's `waiting_on` is shown on
    // its row and moves no other.
    let edges: Vec<(usize, usize)> = open_tasks
        .iter()
        .enumerate()
        .filter(|(_, (_, store, ..))| store.waits == Waits::ByLocalName)
        .flat_map(|(waiter, (.., confirmed))| {
            confirmed
                .iter()
                .flat_map(|record| mentions(&string_text(record.payload.waiting_on.as_str())))
                .filter_map(|mention| match mention {
                    Reference::Todo(blocker) => index.get(&blocker).copied(),
                    Reference::Mail { .. }
                    | Reference::Thread { .. }
                    | Reference::Briefing { .. }
                    | Reference::Proposal(_)
                    | Reference::Batch { .. }
                    | Reference::Target(_) => None,
                })
                .map(move |blocker| (waiter, blocker))
                .collect::<Vec<_>>()
        })
        .collect();
    let readings = waits::readings(&own, &edges);
    let mut rows: Vec<(Option<f64>, usize, &Entry, ListRow)> = open_tasks
        .into_iter()
        .zip(readings)
        .map(|((place, store, entry, record, _), reading)| {
            let row = open_row(entry, record);
            (
                reading.value(entry.value().ok().flatten()),
                place,
                entry,
                ListRow {
                    chain: store.chain.clone(),
                    package: package(store.units, &row.category),
                    row,
                    reading: urgency(reading, &refs),
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
    (rows.into_iter().map(|(.., row)| row).collect(), failed)
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
