//! The one list's laws, over generated stores, waits, revisions and claims: the waits are those
//! of the records the store's policy confirms; the list restricted to a store that no wait
//! touches is that store's list; a store that is absent moves no row it does not reach by waits;
//! a claim the policy does not confirm moves no reading; and the `waiting_on` of a store whose
//! waits are labels moves no reading.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::{Actor, Hash, mk_completed, mk_sealed, seal_hash};
use prodrome_core::fpl;
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Untrusted;
use proptest::prelude::*;
use suzatary_base::chains::ChainName;
use suzatary_plan::listed::{ListRow, Store, Units, Urgency, Waits, listed};
use suzatary_record::decoration::records_of;
use suzatary_record::record::{Envelope, MarkupSource, TodoEvent, mk_authored};
use suzatary_signatures::chain_set::Read;
use suzatary_todo::shown::{Priority, open_rows, percent};

const STORES: [&str; 3] = ["life", "upkeep", "roadmap"];
const PRICES: [f64; 5] = [0.01, 0.3, 0.5, 0.7, 0.9];

fn at(day: u32) -> Datetime {
    Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
}

/// One generated task: its price (an index into [`PRICES`], none for no function), whether it is
/// done, and the tasks it waits on as (store, task) indices.
type Task = (Option<usize>, bool, Vec<(usize, usize)>);

/// Each store's tasks, written by `ada`, and its claims, written by the untrusted `triage`.
#[derive(Debug, Clone)]
struct Stores {
    tasks: Vec<Vec<Task>>,
    claims: Vec<Vec<Task>>,
    over: Vec<Vec<(usize, Over)>>,
    revised: Vec<BTreeMap<usize, Revision>>,
}

/// What `ada` writes over one of her tasks after everything else, at its own price: one revision
/// of its waits, or two that neither descends from, a conflict.
#[derive(Debug, Clone)]
enum Revision {
    After(Vec<(usize, usize)>),
    Conflict(Vec<(usize, usize)>, Vec<(usize, usize)>),
}

impl Revision {
    /// Each revision's waits.
    fn waits(&self) -> Vec<&Vec<(usize, usize)>> {
        match self {
            Revision::After(waits) => vec![waits],
            Revision::Conflict(one, other) => vec![one, other],
        }
    }

    fn map(&self, f: impl Fn(&[(usize, usize)]) -> Vec<(usize, usize)>) -> Revision {
        match self {
            Revision::After(waits) => Revision::After(f(waits)),
            Revision::Conflict(one, other) => Revision::Conflict(f(one), f(other)),
        }
    }
}

/// What `triage` claims over one of `ada`'s tasks: its completion, or a revision at the same
/// price with other waits. The price stays because the core prices an untrusted record's spec,
/// which is no wait's doing.
#[derive(Debug, Clone)]
enum Over {
    Completes,
    Revises(Vec<(usize, usize)>),
}

fn a_task() -> impl Strategy<Value = Task> {
    (
        proptest::option::of(0..PRICES.len()),
        proptest::bool::weighted(0.2),
        proptest::collection::vec((0..STORES.len(), 0usize..6), 0..3),
    )
}

fn stores() -> impl Strategy<Value = Stores> {
    let each =
        || proptest::collection::vec(proptest::collection::vec(a_task(), 0..6), STORES.len());
    let over = proptest::collection::vec(
        proptest::collection::vec(
            (
                0usize..6,
                prop_oneof![
                    Just(Over::Completes),
                    proptest::collection::vec((0..STORES.len(), 0usize..6), 0..3)
                        .prop_map(Over::Revises)
                ],
            ),
            0..4,
        ),
        STORES.len(),
    );
    let waits = || proptest::collection::vec((0..STORES.len(), 0usize..6), 0..3);
    let revision = prop_oneof![
        waits().prop_map(Revision::After),
        (waits(), waits()).prop_map(|(one, other)| Revision::Conflict(one, other)),
    ];
    let revised = proptest::collection::vec(
        proptest::collection::btree_map(0usize..6, revision, 0..4),
        STORES.len(),
    );
    (each(), each(), over, revised).prop_map(|(tasks, claims, over, revised)| Stores {
        tasks,
        claims,
        over,
        revised,
    })
}

fn name(store: usize) -> ChainName {
    ChainName::new(STORES[store]).expect("a slug")
}

fn authored(id: &str, actor: &str, (price, _, waits): &Task) -> TodoEvent {
    let spec = price.map_or_else(fpl::mk_absent, |p| fpl::mk_flat(PRICES[p]).expect("a flat"));
    let waiting: Vec<String> = waits
        .iter()
        .map(|(store, task)| format!("todo:{}/t{task}", STORES[*store]))
        .chain(["Ada".to_owned()])
        .collect();
    mk_authored(
        id,
        at(1),
        actor,
        "todo",
        at(1),
        MarkupSource::escaped(&format!("Do {id}")),
        spec,
        vec![],
        "",
        &waiting.join(", "),
        MarkupSource::empty(),
        None,
        vec![],
        vec![],
        "",
    )
    .expect("a record")
}

/// One store's objects: each task authored with its waits as `todo:` references beside a label,
/// and completed where it is done; then each claim, written by `triage`.
fn objects(
    tasks: &[Task],
    claims: &[Task],
    over: &[(usize, Over)],
    revised: &BTreeMap<usize, Revision>,
) -> Vec<(Hash, Envelope)> {
    let mut events = Vec::new();
    for (index, task) in tasks.iter().enumerate() {
        let id = format!("t{index}");
        events.push(authored(&id, "ada", task));
        if task.1 {
            events.push(mk_completed(&id, at(2), "ada", "").expect("a completion"));
        }
    }
    for (index, claim) in claims.iter().enumerate() {
        events.push(authored(&format!("c{index}"), "triage", claim));
    }
    for (task, over) in over.iter().filter(|(task, _)| *task < tasks.len()) {
        let id = format!("t{task}");
        events.push(match over {
            Over::Completes => mk_completed(&id, at(2), "triage", "").expect("a claim"),
            Over::Revises(waits) => {
                authored(&id, "triage", &(tasks[*task].0, false, waits.clone()))
            }
        });
    }
    let mut objects: Vec<(Hash, Envelope)> = Vec::new();
    for event in events {
        let envelope = mk_sealed(objects.last().map(|(name, _)| name.clone()), event);
        objects.push((seal_hash(&envelope), envelope));
    }
    // Every revision rests on the same tip, so two of one task are a conflict.
    let tip = objects.last().map(|(name, _)| name.clone());
    for (task, revision) in revised.iter().filter(|(task, _)| **task < tasks.len()) {
        for waits in revision.waits() {
            let event = authored(
                &format!("t{task}"),
                "ada",
                &(tasks[*task].0, false, waits.clone()),
            );
            let envelope = mk_sealed(tip.clone(), event);
            objects.push((seal_hash(&envelope), envelope));
        }
    }
    objects
}

fn read(generated: &Stores, held: &BTreeSet<usize>) -> Read {
    let triage = Untrusted::of([Actor::new("triage").expect("an actor")]);
    Read::of_objects(
        (0..STORES.len())
            .filter(|store| held.contains(store))
            .map(|store| {
                let objects = objects(
                    &generated.tasks[store],
                    &generated.claims[store],
                    &generated.over[store],
                    &generated.revised[store],
                );
                (name(store), triage.clone(), objects)
            }),
    )
}

const PRIVATE: [Waits; 3] = [Waits::ByLocalName; 3];

fn the_list(read: &Read, waits: [Waits; 3]) -> Vec<ListRow> {
    let names: Vec<ChainName> = (0..STORES.len()).map(name).collect();
    let stores: Vec<Store<'_>> = names
        .iter()
        .zip(waits)
        .map(|(chain, waits)| Store {
            chain,
            waits,
            units: Units::Unknown,
        })
        .collect();
    listed(&read.set(at(3)), &stores).0
}

fn all() -> BTreeSet<usize> {
    (0..STORES.len()).collect()
}

fn by_key(list: &[ListRow]) -> BTreeMap<(ChainName, String), &ListRow> {
    list.iter()
        .map(|row| ((row.chain.clone(), row.row.id.clone()), row))
        .collect()
}

/// `generated` with every wait into or out of `store` taken away, the claims' included.
fn untouched(generated: &Stores, store: usize) -> Stores {
    let cut = |stores: &[Vec<Task>]| -> Vec<Vec<Task>> {
        stores
            .iter()
            .enumerate()
            .map(|(here, tasks)| {
                tasks
                    .iter()
                    .map(|(price, done, waits)| {
                        let kept = waits
                            .iter()
                            .filter(|(there, _)| here != store && *there != store)
                            .copied()
                            .collect();
                        (*price, *done, kept)
                    })
                    .collect()
            })
            .collect()
    };
    let kept = |here: usize, waits: &[(usize, usize)]| -> Vec<(usize, usize)> {
        waits
            .iter()
            .filter(|(there, _)| here != store && *there != store)
            .copied()
            .collect()
    };
    Stores {
        tasks: cut(&generated.tasks),
        claims: cut(&generated.claims),
        over: generated.over.clone(),
        revised: generated
            .revised
            .iter()
            .enumerate()
            .map(|(here, revised)| {
                revised
                    .iter()
                    .map(|(task, revision)| (*task, revision.map(|waits| kept(here, waits))))
                    .collect()
            })
            .collect(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Restricted to a store that no wait touches, with waits among the other stores, the one
    /// list is that store's own list, in its order.
    #[test]
    fn the_one_list_restricted_to_a_store_is_its_list(generated in stores()) {
        for store in 0..STORES.len() {
            let read = read(&untouched(&generated, store), &all());
            let list = the_list(&read, PRIVATE);
            let set = read.set(at(3));
            let chain = name(store);
            let restricted: Vec<_> =
                list.iter().filter(|row| row.chain == chain).map(|row| row.row.clone()).collect();
            let nodes = set.held(&chain).expect("held").nodes;
            let own = open_rows(&set.entries(&chain).expect("rows"), &records_of(nodes));
            prop_assert_eq!(restricted, own);
        }
    }

    /// A store that is absent changes no row of another store that its tasks do not reach by
    /// waits, and the list still holds every other store's open tasks.
    #[test]
    fn an_absent_store_moves_only_the_rows_it_reaches(
        generated in stores(),
        gone in 0..STORES.len(),
    ) {
        let whole = the_list(&read(&generated, &all()), PRIVATE);
        let mut held = all();
        held.remove(&gone);
        let without = the_list(&read(&generated, &held), PRIVATE);
        let reached = reached_from(&generated, gone);
        let after = by_key(&without);
        for (key, row) in by_key(&whole).into_iter().filter(|((chain, _), _)| *chain != name(gone)) {
            let moved = after.get(&key);
            prop_assert!(moved.is_some(), "{:?} left the list", key);
            if !reached.contains(&key) {
                prop_assert_eq!(moved.copied(), Some(row));
            }
        }
        prop_assert!(without.iter().all(|row| row.chain != name(gone)));
    }

    /// Claims written by an actor the policy does not confirm, new todos or completions and revisions
    /// of a trusted task, move no reading either way: every task reads as it reads with no
    /// claims at all, and is still on the list.
    #[test]
    fn a_claim_the_policy_does_not_confirm_moves_no_reading(generated in stores()) {
        let none = Stores {
            claims: vec![Vec::new(); STORES.len()],
            over: vec![Vec::new(); STORES.len()],
            ..generated.clone()
        };
        let without = the_list(&read(&none, &all()), PRIVATE);
        let with = the_list(&read(&generated, &all()), PRIVATE);
        let with = by_key(&with);
        for (key, row) in by_key(&without) {
            let claimed = with.get(&key).copied();
            prop_assert!(claimed.is_some(), "{:?} left the list", key);
            prop_assert_eq!(
                claimed.map(|row| (&row.reading, &row.row.priority)),
                Some((&row.reading, &row.row.priority)),
                "{:?}", key
            );
        }
    }

    /// The waits are those of the records the store's policy confirms: a task's own, or the
    /// later revision's that replaced them (so a revision that clears a wait ends it), or each
    /// candidate's of two revisions in conflict; whatever is claimed over it. Each task reads
    /// the least price upstream of it under exactly those waits.
    #[test]
    fn the_waits_are_those_of_the_confirmed_records(generated in stores()) {
        let open = |store: usize, task: usize| {
            generated.tasks[store].get(task).is_some_and(|(_, done, _)| !*done)
        };
        let waits = |store: usize, task: usize| -> Vec<(usize, usize)> {
            match generated.revised[store].get(&task) {
                Some(revision) => revision.waits().into_iter().flatten().copied().collect(),
                None => generated.tasks[store][task].2.clone(),
            }
        };
        let own = |(store, task): (usize, usize)| {
            generated.tasks[store][task].0.map(|price| PRICES[price])
        };
        let list = the_list(&read(&generated, &all()), PRIVATE);
        let rows = by_key(&list);
        for (store, label) in STORES.iter().enumerate() {
            for task in (0..generated.tasks[store].len()).filter(|task| open(store, *task)) {
                let mut upstream = BTreeSet::from([(store, task)]);
                loop {
                    let more: Vec<(usize, usize)> = (0..STORES.len())
                        .flat_map(|s| (0..generated.tasks[s].len()).map(move |t| (s, t)))
                        .filter(|(s, t)| open(*s, *t) && !upstream.contains(&(*s, *t)))
                        .filter(|(s, t)| waits(*s, *t).iter().any(|blocker| upstream.contains(blocker)))
                        .collect();
                    if more.is_empty() {
                        break;
                    }
                    upstream.extend(more);
                }
                let least = upstream.iter().filter_map(|key| own(*key)).min_by(f64::total_cmp);
                let row = rows.get(&(name(store), format!("t{task}")));
                prop_assert!(row.is_some(), "{} t{} is open", label, task);
                let read = row.map(|row| match &row.reading {
                    Urgency::Raised { percent, .. } => Some(percent.clone()),
                    Urgency::Own => match &row.row.priority {
                        Priority::Percent { percent } => Some(percent.clone()),
                        Priority::Absent | Priority::Unlinked => None,
                    },
                });
                prop_assert_eq!(read, Some(least.map(percent)), "{} t{}", label, task);
            }
        }
    }

    /// In a store whose waits are labels (a public or a followed one), no `waiting_on` moves any
    /// reading: every row reads as it does with that store's waits taken away.
    #[test]
    fn a_label_moves_no_reading(generated in stores(), labels in 0..STORES.len()) {
        let mut modes = PRIVATE;
        modes[labels] = Waits::None;
        let bare = |stores: &[Vec<Task>]| -> Vec<Vec<Task>> {
            stores.iter().enumerate().map(|(here, tasks)| {
                tasks.iter().map(|(price, done, waits)| {
                    (*price, *done, if here == labels { Vec::new() } else { waits.clone() })
                }).collect()
            }).collect()
        };
        let quiet = Stores {
            tasks: bare(&generated.tasks),
            claims: bare(&generated.claims),
            over: generated.over.clone(),
            revised: generated.revised.iter().enumerate().map(|(here, revised)| {
                revised.iter().map(|(task, revision)| {
                    (*task, revision.map(|waits| if here == labels { Vec::new() } else { waits.to_vec() }))
                }).collect()
            }).collect(),
        };
        let reading = |list: &[ListRow]| -> Vec<_> {
            list.iter().map(|row| (row.chain.clone(), row.row.id.clone(), row.reading.clone())).collect()
        };
        prop_assert_eq!(
            reading(&the_list(&read(&generated, &all()), modes)),
            reading(&the_list(&read(&quiet, &all()), modes))
        );
    }
}

/// Every open task of another store that a task of `gone` reaches by waits.
fn reached_from(generated: &Stores, gone: usize) -> BTreeSet<(ChainName, String)> {
    let edges: Vec<((usize, usize), (usize, usize))> = generated
        .tasks
        .iter()
        .enumerate()
        .flat_map(|(store, tasks)| {
            tasks
                .iter()
                .enumerate()
                .flat_map(move |(index, (_, _, waits))| {
                    waits.iter().map(move |blocker| ((store, index), *blocker))
                })
        })
        .chain(
            generated
                .revised
                .iter()
                .enumerate()
                .flat_map(|(store, revised)| {
                    revised.iter().flat_map(move |(task, revision)| {
                        revision
                            .waits()
                            .into_iter()
                            .flatten()
                            .map(move |blocker| ((store, *task), *blocker))
                    })
                }),
        )
        .collect();
    let mut reached: BTreeSet<(usize, usize)> = (0..generated.tasks[gone].len())
        .map(|index| (gone, index))
        .collect();
    loop {
        let more: Vec<(usize, usize)> = edges
            .iter()
            .filter(|(w, b)| reached.contains(w) && !reached.contains(b))
            .map(|(_, b)| *b)
            .collect();
        if more.is_empty() {
            break;
        }
        reached.extend(more);
    }
    reached
        .into_iter()
        .map(|(store, index)| (name(store), format!("t{index}")))
        .collect()
}
