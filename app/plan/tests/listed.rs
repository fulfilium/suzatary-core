//! The one list's laws, over generated stores, waits, revisions and claims: the waits are those
//! of the records the store's policy confirms; the list restricted to a store that no wait
//! touches is that store's list; a store that is absent moves no row it does not reach by waits;
//! a claim the policy does not confirm moves no reading; and the `waiting_on` of a store whose
//! waits are labels moves no reading. Where a store spells its waits by genesis: the reading is
//! unchanged by renaming every local chain, and a wait into an absent store moves nothing and is
//! said. Words in a store anyone reads move no private chain's row, two held stores that begin
//! with one genesis are two stores, and every law draws both shapes of store, one begun by a
//! genesis and a legacy one.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::change::mk_change;
use prodrome_core::event::{Actor, Hash, mk_completed, mk_created, mk_sealed, seal_hash};
use prodrome_core::fpl;
use prodrome_core::genesis::mk_genesis;
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Untrusted;
use proptest::prelude::*;
use suzatary_base::chains::ChainName;
use suzatary_base::reference::{Reference, TodoRef};
use suzatary_plan::listed::{ListRow, Store, Units, Unresolved, Urgency, listed};
use suzatary_plan::wait::{Audience, Kept, Missing};
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

/// Each store's tasks, written by `ada`, and its claims, written by the untrusted `triage`, each
/// wait spelled as its store spells them.
#[derive(Debug, Clone)]
struct Stores {
    shapes: Vec<Shape>,
    /// Whether the last store begins with the second's first object: a replica of it.
    replica: bool,
    spelled: Vec<Spelling>,
    tasks: Vec<Vec<Task>>,
    claims: Vec<Vec<Task>>,
    over: Vec<Vec<(usize, Over)>>,
    revised: Vec<BTreeMap<usize, Revision>>,
}

/// A store begun by a genesis, its writes changes over each todo's heads; or a legacy one, a line
/// of sealed objects from a first that carries no task, named by that root.
#[derive(Debug, Clone, Copy)]
enum Shape {
    Genesis,
    Legacy,
}

/// How a store spells the tasks it waits on: by this deployment's chain names, or by the first
/// `n` hex of each store's genesis.
#[derive(Debug, Clone, Copy)]
enum Spelling {
    Local,
    Genesis(usize),
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
    let shape = prop_oneof![Just(Shape::Genesis), Just(Shape::Legacy)];
    (
        (each(), each(), over, revised),
        proptest::collection::vec(shape, STORES.len()),
        proptest::bool::weighted(0.3),
    )
        .prop_map(|((tasks, claims, over, revised), shapes, replica)| Stores {
            shapes,
            replica,
            spelled: vec![Spelling::Local; STORES.len()],
            tasks,
            claims,
            over,
            revised,
        })
}

/// [`stores`], each store spelling its waits by local name or by a genesis prefix.
fn spelled_stores() -> impl Strategy<Value = Stores> {
    let spelling = prop_oneof![
        Just(Spelling::Local),
        (8usize..=64).prop_map(Spelling::Genesis)
    ];
    (stores(), proptest::collection::vec(spelling, STORES.len())).prop_map(
        |(generated, spelled)| Stores {
            spelled,
            ..generated
        },
    )
}

fn name(store: usize) -> ChainName {
    ChainName::new(STORES[store]).expect("a slug")
}

/// The store whose first object `store` begins with: itself, or the second for a replica.
fn origin(generated: &Stores, store: usize) -> usize {
    if generated.replica && store == STORES.len() - 1 {
        1
    } else {
        store
    }
}

/// Each store's first object, the same whatever the deployment calls the store and whatever it
/// holds after.
fn first(generated: &Stores, store: usize) -> Envelope {
    let origin = origin(generated, store);
    match generated.shapes[origin] {
        Shape::Genesis => {
            Envelope::Genesis(mk_genesis("todos", &format!("{origin:032x}")).expect("a genesis"))
        }
        Shape::Legacy => mk_sealed(
            None,
            mk_created(&format!("root{origin}"), at(1), "ada", "the root", "").expect("a creation"),
        ),
    }
}

/// The prodrome a store begins: its genesis, or a legacy store's root.
fn home(generated: &Stores, store: usize) -> Hash {
    seal_hash(&first(generated, store))
}

/// How a store, of a deployment that calls the stores `names`, writes its waits.
#[derive(Clone, Copy)]
struct Writing<'a> {
    spelling: Spelling,
    names: &'a [&'a str],
    homes: &'a [Hash],
}

fn authored(id: &str, actor: &str, (price, _, waits): &Task, by: Writing<'_>) -> TodoEvent {
    let spec = price.map_or_else(fpl::mk_absent, |p| fpl::mk_flat(PRICES[p]).expect("a flat"));
    let waiting: Vec<String> = waits
        .iter()
        .map(|(store, task)| match by.spelling {
            Spelling::Local => format!("todo:{}/t{task}", by.names[*store]),
            Spelling::Genesis(n) => {
                format!("todo:+{}/t{task}", &by.homes[*store].as_str()[..n])
            }
        })
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

/// The store `store`'s objects: its genesis; each task authored with its waits as `todo:`
/// references beside a label, and completed where it is done; then each claim, written by
/// `triage`. Each change rests on its todo's heads.
fn objects(generated: &Stores, store: usize, names: &[&str]) -> Vec<(Hash, Envelope)> {
    let homes: Vec<Hash> = (0..STORES.len()).map(|s| home(generated, s)).collect();
    let writing = Writing {
        spelling: generated.spelled[store],
        names,
        homes: &homes,
    };
    let (tasks, revised) = (&generated.tasks[store], &generated.revised[store]);
    let mut events = Vec::new();
    for (index, task) in tasks.iter().enumerate() {
        let id = format!("t{index}");
        events.push(authored(&id, "ada", task, writing));
        if task.1 {
            events.push(mk_completed(&id, at(2), "ada", "").expect("a completion"));
        }
    }
    for (index, claim) in generated.claims[store].iter().enumerate() {
        events.push(authored(&format!("c{index}"), "triage", claim, writing));
    }
    for (task, over) in generated.over[store]
        .iter()
        .filter(|(task, _)| *task < tasks.len())
    {
        let id = format!("t{task}");
        events.push(match over {
            Over::Completes => mk_completed(&id, at(2), "triage", "").expect("a claim"),
            Over::Revises(waits) => authored(
                &id,
                "triage",
                &(tasks[*task].0, false, waits.clone()),
                writing,
            ),
        });
    }
    let begun = first(generated, store);
    let home = seal_hash(&begun);
    let mut objects: Vec<(Hash, Envelope)> = vec![(home.clone(), begun)];
    let shape = generated.shapes[origin(generated, store)];
    // A change rests on its todo's heads; a legacy object on the last one.
    let mut heads: BTreeMap<String, Vec<Hash>> = BTreeMap::new();
    let put = |event: TodoEvent, deps: Vec<Hash>, objects: &mut Vec<(Hash, Envelope)>| {
        let envelope = match shape {
            Shape::Genesis => {
                Envelope::Change(mk_change(home.clone(), deps, event).expect("a change"))
            }
            Shape::Legacy => mk_sealed(deps.into_iter().next(), event),
        };
        let name = seal_hash(&envelope);
        objects.push((name.clone(), envelope));
        name
    };
    let deps =
        |heads: &BTreeMap<String, Vec<Hash>>, id: &str, objects: &[(Hash, Envelope)]| match shape {
            Shape::Genesis => heads.get(id).cloned().unwrap_or_default(),
            Shape::Legacy => objects
                .last()
                .map(|(name, _)| name.clone())
                .into_iter()
                .collect(),
        };
    for event in events {
        let id = event.todo().as_str().to_owned();
        let on = deps(&heads, &id, &objects);
        let name = put(event, on, &mut objects);
        heads.insert(id, vec![name]);
    }
    // Every revision of a task rests on the same heads, so two of one task are a conflict.
    for (task, revision) in revised.iter().filter(|(task, _)| **task < tasks.len()) {
        let id = format!("t{task}");
        let on = deps(&heads, &id, &objects);
        for waits in revision.waits() {
            let event = authored(&id, "ada", &(tasks[*task].0, false, waits.clone()), writing);
            put(event, on.clone(), &mut objects);
        }
    }
    objects
}

fn read(generated: &Stores, held: &BTreeSet<usize>) -> Read {
    read_named(generated, held, &STORES)
}

/// The stores of `held`, the deployment calling store `i` by `names[i]`.
fn read_named(generated: &Stores, held: &BTreeSet<usize>, names: &[&str]) -> Read {
    let triage = Untrusted::of([Actor::new("triage").expect("an actor")]);
    Read::of_objects(
        (0..STORES.len())
            .filter(|store| held.contains(store))
            .map(|store| {
                let chain = ChainName::new(names[store]).expect("a slug");
                (chain, triage.clone(), objects(generated, store, names))
            }),
    )
}

const PRIVATE: [Kept; 3] = [Kept::Private; 3];

fn the_list(read: &Read, audiences: [Kept; 3]) -> Vec<ListRow> {
    the_named_list(read, audiences, &STORES).rows
}

fn the_named_list(
    read: &Read,
    audiences: [Kept; 3],
    names: &[&str],
) -> suzatary_plan::listed::Listed {
    let names: Vec<ChainName> = names
        .iter()
        .map(|name| ChainName::new(*name).expect("a slug"))
        .collect();
    let stores: Vec<Store<'_>> = names
        .iter()
        .zip(audiences)
        .map(|(chain, kept)| Store {
            chain,
            kept,
            units: Units::Unknown,
        })
        .collect();
    listed(&read.set(at(3)), &stores)
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
        shapes: generated.shapes.clone(),
        replica: generated.replica,
        spelled: generated.spelled.clone(),
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
    fn a_claim_the_policy_does_not_confirm_moves_no_reading(
        generated in spelled_stores(),
        audiences in audiences(),
    ) {
        let none = Stores {
            claims: vec![Vec::new(); STORES.len()],
            over: vec![Vec::new(); STORES.len()],
            ..generated.clone()
        };
        let without = the_list(&read(&none, &all()), audiences);
        let with = the_list(&read(&generated, &all()), audiences);
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
    /// the least price upstream of it under exactly those waits, each read by the rule
    /// ([`counted`]).
    #[test]
    fn the_waits_are_those_of_the_confirmed_records(
        generated in spelled_stores(),
        audiences in audiences(),
    ) {
        let open = |store: usize, task: usize| {
            generated.tasks[store].get(task).is_some_and(|(_, done, _)| !*done)
        };
        let waits = |store: usize, task: usize| -> Vec<(usize, usize)> {
            confirmed(&generated, store, task)
                .into_iter()
                .filter_map(|blocker| match counted(&generated, audiences, &all(), store, blocker) {
                    Counted::Into(there) => Some((there, blocker.1)),
                    Counted::Label | Counted::Said(_) => None,
                })
                .collect()
        };
        let own = |(store, task): (usize, usize)| {
            generated.tasks[store][task].0.map(|price| PRICES[price])
        };
        let listed = the_named_list(&read(&generated, &all()), audiences, &STORES);
        let mut said = BTreeSet::new();
        for store in 0..STORES.len() {
            for task in (0..generated.tasks[store].len()).filter(|task| open(store, *task)) {
                for blocker in confirmed(&generated, store, task) {
                    if let Counted::Said(missing) = counted(&generated, audiences, &all(), store, blocker) {
                        said.insert((
                            TodoRef { chain: name(store), id: todo_id(task) },
                            spelled(&generated, store, blocker),
                            missing,
                        ));
                    }
                }
            }
        }
        let unresolved: BTreeSet<_> = listed
            .unresolved
            .iter()
            .map(|wait| (wait.waiter.clone(), wait.reference.to_string(), wait.missing.clone()))
            .collect();
        prop_assert_eq!(unresolved, said);
        let list = listed.rows;
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
        modes[labels] = Kept::Public;
        let bare = |stores: &[Vec<Task>]| -> Vec<Vec<Task>> {
            stores.iter().enumerate().map(|(here, tasks)| {
                tasks.iter().map(|(price, done, waits)| {
                    (*price, *done, if here == labels { Vec::new() } else { waits.clone() })
                }).collect()
            }).collect()
        };
        let quiet = Stores {
            shapes: generated.shapes.clone(),
            replica: generated.replica,
            spelled: generated.spelled.clone(),
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

    /// The reading is unchanged by renaming every local chain: a private store's local names are
    /// the deployment's and follow it, and a name by genesis is the same under any.
    #[test]
    fn renaming_every_local_chain_changes_no_reading(
        generated in spelled_stores(),
        audiences in audiences(),
    ) {
        const RENAMED: [&str; 3] = ["alpha", "beta", "gamma"];
        let reading = |names: &[&str]| -> Vec<_> {
            let place = |chain: &ChainName| names.iter().position(|name| *name == chain.as_str());
            the_named_list(&read_named(&generated, &all(), names), audiences, names)
                .rows
                .iter()
                .map(|row| {
                    let by = match &row.reading {
                        Urgency::Own => None,
                        Urgency::Raised { percent, by } => {
                            Some((percent.clone(), place(&by.chain), by.id.clone()))
                        }
                    };
                    (place(&row.chain), row.row.id.clone(), row.name.clone(), by)
                })
                .collect()
        };
        prop_assert_eq!(reading(&STORES), reading(&RENAMED));
    }

    /// A wait into a store that is absent moves no row it does not reach, and is said: every
    /// confirmed wait of an open task on a task of the absent store, by either spelling, is in
    /// `unresolved` as unheld, and nothing else is.
    #[test]
    fn a_wait_into_an_absent_store_moves_nothing_and_is_said(
        generated in spelled_stores().prop_map(|generated| Stores { replica: false, ..generated }),
        gone in 0..STORES.len(),
    ) {
        let mut held = all();
        held.remove(&gone);
        let whole = the_list(&read(&generated, &all()), PRIVATE);
        let without = the_named_list(&read(&generated, &held), PRIVATE, &STORES);
        let reached = reached_from(&generated, gone);
        let after = by_key(&without.rows);
        for (key, row) in by_key(&whole).into_iter().filter(|((chain, _), _)| *chain != name(gone)) {
            if !reached.contains(&key) {
                prop_assert_eq!(after.get(&key).copied(), Some(row));
            }
        }
        let mut expected = BTreeSet::new();
        for store in held.iter().copied() {
            for (task, (_, done, _)) in generated.tasks[store].iter().enumerate() {
                let confirmed = confirmed(&generated, store, task);
                for (there, blocker) in confirmed.into_iter().filter(|(there, _)| *there == gone) {
                    if !*done {
                        expected.insert((
                            TodoRef { chain: name(store), id: todo_id(task) },
                            (there, format!("t{blocker}")),
                        ));
                    }
                }
            }
        }
        let said: BTreeSet<_> = without
            .unresolved
            .iter()
            .map(|Unresolved { waiter, reference, missing }| {
                assert_eq!(*missing, suzatary_plan::wait::Missing::Unheld);
                (waiter.clone(), named(&generated, reference))
            })
            .collect();
        prop_assert_eq!(said, expected);
    }

    /// Words anyone may write move no private chain's row: whatever a public or a followed store
    /// holds, its genesis and name kept, every row of a private chain reads the same.
    #[test]
    fn no_store_anyone_reads_moves_a_private_row(
        generated in spelled_stores(),
        other in spelled_stores(),
        audiences in audiences(),
    ) {
        let swapped = |mine: &Stores| {
            let pick = |store: usize| if audiences[store] == Kept::Private { mine } else { &other };
            Stores {
                spelled: (0..STORES.len()).map(|s| pick(s).spelled[s]).collect(),
                tasks: (0..STORES.len()).map(|s| pick(s).tasks[s].clone()).collect(),
                claims: (0..STORES.len()).map(|s| pick(s).claims[s].clone()).collect(),
                over: (0..STORES.len()).map(|s| pick(s).over[s].clone()).collect(),
                revised: (0..STORES.len()).map(|s| pick(s).revised[s].clone()).collect(),
                ..mine.clone()
            }
        };
        let private = |list: Vec<ListRow>| -> Vec<_> {
            list.into_iter()
                .filter(|row| audiences[STORES.iter().position(|n| *n == row.chain.as_str()).expect("a store")] == Kept::Private)
                .map(|row| (row.chain, row.row.id, row.reading))
                .collect()
        };
        prop_assert_eq!(
            private(the_list(&read(&generated, &all()), audiences)),
            private(the_list(&read(&swapped(&generated), &all()), audiences))
        );
    }
}

fn audiences() -> impl Strategy<Value = [Kept; 3]> {
    proptest::array::uniform3(prop_oneof![
        Just(Kept::Private),
        Just(Kept::Public),
        Just(Kept::Followed)
    ])
}

/// The waits of the records `store`'s policy confirms for `task`: the last revision's, each
/// candidate's of a conflict, else its own.
fn confirmed(generated: &Stores, store: usize, task: usize) -> Vec<(usize, usize)> {
    match generated.revised[store].get(&task) {
        Some(revision) => revision.waits().into_iter().flatten().copied().collect(),
        None => generated.tasks[store][task].2.clone(),
    }
}

/// What a generated wait does, by the rule written out apart from the code.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Counted {
    Into(usize),
    Label,
    Said(Missing),
}

/// A wait written in `here` on the task `(there, _)`, spelled as `here` spells waits, among the
/// stores `held`, each kept as `kept` says. A prefix two held geneses share is drawn by the rule's
/// own law (`wait.rs`), since a store's genesis cannot be chosen to share one here.
fn counted(
    generated: &Stores,
    kept: [Kept; 3],
    held: &BTreeSet<usize>,
    here: usize,
    (there, _): (usize, usize),
) -> Counted {
    let anyone = kept[here].audience() == Audience::Anyone;
    match generated.spelled[here] {
        Spelling::Local if anyone => Counted::Label,
        Spelling::Local if held.contains(&there) => Counted::Into(there),
        Spelling::Local => Counted::Said(Missing::Unheld),
        Spelling::Genesis(_) => {
            let holders: Vec<usize> = held
                .iter()
                .copied()
                .filter(|s| home(generated, *s) == home(generated, there))
                .collect();
            let readable: Vec<usize> = holders
                .iter()
                .copied()
                .filter(|s| !anyone || kept[*s].audience() == Audience::Anyone)
                .collect();
            let owned: Vec<usize> = readable
                .iter()
                .copied()
                .filter(|s| kept[*s].own())
                .collect();
            match (holders.as_slice(), owned.as_slice(), readable.as_slice()) {
                ([], ..) => Counted::Said(Missing::Unheld),
                (_, _, []) => Counted::Said(Missing::Private),
                (_, [one], _) | (_, _, [one]) => Counted::Into(*one),
                (.., several) => Counted::Said(Missing::Replicas {
                    stores: several.iter().map(|s| name(*s)).collect(),
                }),
            }
        }
    }
}

/// The text a generated wait on `(there, task)` is written as in `here`.
fn spelled(generated: &Stores, here: usize, (there, task): (usize, usize)) -> String {
    match generated.spelled[here] {
        Spelling::Local => format!("todo:{}/t{task}", STORES[there]),
        Spelling::Genesis(n) => format!("todo:+{}/t{task}", &home(generated, there).as_str()[..n]),
    }
}

fn todo_id(task: usize) -> prodrome_core::event::TodoId {
    prodrome_core::event::TodoId::new(format!("t{task}")).expect("an id")
}

/// The (store, id) a generated wait's reference names, by either spelling.
fn named(generated: &Stores, reference: &Reference) -> (usize, String) {
    match reference {
        Reference::Todo(local) => (
            STORES
                .iter()
                .position(|name| *name == local.chain.as_str())
                .expect("a store"),
            local.id.as_str().to_owned(),
        ),
        Reference::Global(global) => (
            (0..STORES.len())
                .find(|store| {
                    home(generated, *store)
                        .as_str()
                        .starts_with(global.genesis.as_str())
                })
                .expect("a store"),
            global.id.as_str().to_owned(),
        ),
        other => panic!("a generated wait is a todo, got {other}"),
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
