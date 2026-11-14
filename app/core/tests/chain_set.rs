use std::collections::BTreeMap as Specs;

use prodrome_core::event::{Envelope, Hash, mk_sealed, mk_spec_revised, seal_hash};
use prodrome_core::fpl::{self, Env, link_in, mk_flat, mk_ref_in};
use prodrome_core::literal::Datetime;
use prodrome_core::term::Term;

use prodrome_core::dag::Dag;
use prodrome_core::observe::{NextChange, Observation};
use prodrome_core::policy::Untrusted;
use suzatary_base::chains::ChainName;
use suzatary_core::stores::InMemory;
use suzatary_record::record::TodoEvent;
use suzatary_signatures::chain_set::*;
use suzatary_store::store::Prodrome as _;
use suzatary_store::store::Reads as _;

type Objects = Vec<(Hash, Envelope<TodoEvent>)>;

fn at(hour: u32) -> Datetime {
    Datetime::new(2026, 10, 4, hour, 0, 0, 0).expect("an instant")
}

fn spec(todo: &str, term: Term, hour: u32) -> TodoEvent {
    mk_spec_revised(todo, at(hour), "ada", term, "").expect("an event")
}

fn priced(todo: &str, value: f64, hour: u32) -> TodoEvent {
    spec(todo, mk_flat(value).expect("a flat"), hour)
}

fn name(text: &str) -> ChainName {
    ChainName::new(text).expect("a slug")
}

fn legacy(events: Vec<TodoEvent>) -> Objects {
    let mut objects: Objects = Vec::new();
    for event in events {
        let object = mk_sealed(objects.last().map(|(name, _)| name.clone()), event);
        objects.push((seal_hash(&object), object));
    }
    objects
}

fn begun(events: Vec<TodoEvent>) -> Objects {
    let store = InMemory::<TodoEvent>::default();
    for event in events {
        store.append(event).expect("appends");
    }
    store.objects().expect("reads")
}

fn two_geneses() -> Objects {
    ["one", "two"]
        .into_iter()
        .map(|label| {
            let genesis: Envelope<TodoEvent> = Envelope::Genesis(
                prodrome_core::genesis::mk_genesis(label, &"0".repeat(32)).expect("a genesis"),
            );
            (seal_hash(&genesis), genesis)
        })
        .collect()
}

fn read(chains: Vec<(&str, Objects)>, untrusted: &Untrusted) -> Read {
    Read::of_objects(
        chains
            .into_iter()
            .map(|(chain, objects)| (name(chain), untrusted.clone(), objects)),
    )
}

/// What `RefIn(store, todo)` reads through `set` at its instant.
fn through(set: &ChainSet<'_>, store: &str, todo: &str) -> Option<f64> {
    let term: Term = mk_ref_in(store.to_owned(), todo.to_owned()).expect("a reference");
    let closed = link_in(&term, &Specs::new(), set.stores()).expect("links");
    fpl::fulfillment(&closed, set.instant(), &Env::new())
}

fn row(set: &ChainSet<'_>, chain: &str, todo: &str) -> Option<f64> {
    set.entries(&name(chain))
        .expect("rows")
        .iter()
        .find(|row| row.key.as_str() == todo)
        .and_then(|row| row.value().expect("links"))
}

/// A legacy chain and a chain a `Genesis` begins are both members, each by its name.
#[test]
fn every_chain_is_a_member_by_its_name() {
    let project = begun(vec![priced("alpha", 0.6, 1)]);
    let genesis = Dag::from_iter(project.clone()).geneses().into_iter().next();
    let chains = read(
        vec![
            ("life", legacy(vec![priced("permit", 0.3, 1)])),
            ("suzatary", project),
        ],
        &Untrusted::none(),
    );
    let set = chains.set(at(2));
    assert_eq!(through(&set, "life", "permit"), Some(0.3));
    assert_eq!(through(&set, "suzatary", "alpha"), Some(0.6));
    assert_eq!(through(&set, "life", "alpha"), None, "another chain's todo");
    assert_eq!(
        through(&set, "elsewhere", "permit"),
        None,
        "a chain not held"
    );
    let genesis = genesis.expect("begun");
    assert_eq!(
        through(&set, genesis.as_str(), "alpha"),
        Some(0.6),
        "a member by its genesis too"
    );
}

/// Each chain is read under its own policy: an untrusted writer prices nothing in its chain.
#[test]
fn each_chain_reads_under_its_own_policy() {
    let mut life = legacy(vec![priced("permit", 0.3, 1)]);
    let claimed = mk_sealed(
        Some(life[0].0.clone()),
        mk_spec_revised("permit", at(2), "triage", mk_flat(0.1).expect("a flat"), "")
            .expect("an event"),
    );
    life.push((seal_hash(&claimed), claimed));
    let read_under = |untrusted: &Untrusted| {
        let chains = read(vec![("life", life.clone())], untrusted);
        through(&chains.set(at(3)), "life", "permit")
    };
    assert_eq!(read_under(&Untrusted::none()), Some(0.1));
    let triage = prodrome_core::event::Actor::new("triage").expect("an actor");
    assert_eq!(read_under(&Untrusted::of([triage])), Some(0.3));
}

/// A chain holding two geneses is no member: it is held as that failure, a reference into it
/// reads `∅`, and no other chain's price moves.
#[test]
fn a_broken_member_moves_nothing_that_does_not_refer_into_it() {
    assert_eq!(home(&Dag::from_iter(two_geneses())), Err(2));
    let life = legacy(vec![
        priced("permit", 0.3, 1),
        spec(
            "fee",
            fpl::mk_offset(
                0.1,
                mk_ref_in("broken".to_owned(), "x".to_owned()).expect("a ref"),
            )
            .expect("an offset"),
            1,
        ),
    ]);
    let none = Untrusted::none();
    let alone = read(vec![("life", life.clone())], &none);
    let beside = read(vec![("life", life), ("broken", two_geneses())], &none);
    let (alone, beside) = (alone.set(at(2)), beside.set(at(2)));
    assert_eq!(
        row(&beside, "life", "permit"),
        row(&alone, "life", "permit")
    );
    assert_eq!(row(&beside, "life", "fee"), row(&alone, "life", "fee"));
    assert_eq!(through(&beside, "broken", "x"), None);
    assert!(matches!(
        beside.failures().collect::<Vec<_>>().as_slice(),
        [NotAMember::Several(_, 2)]
    ));
}

/// A write to one chain re-reads that chain alone: the other is the earlier read's own, its
/// readings kept, and the set reads at every instant as a fresh read of both stores does.
#[test]
fn a_write_to_one_chain_rereads_that_chain_alone() {
    let policy = |chain: &str| suzatary_base::chains::ChainPolicy {
        name: name(chain),
        untrusted: Untrusted::none(),
        roster: std::collections::BTreeSet::new(),
        root: suzatary_core::cast::keys::root(),
    };
    let (life, project) = (policy("life"), policy("suzatary"));
    let (life_store, project_store) = (
        InMemory::<TodoEvent>::default(),
        InMemory::<TodoEvent>::default(),
    );
    let term = fpl::mk_offset(
        0.1,
        mk_ref_in("suzatary".to_owned(), "alpha".to_owned()).expect("a ref"),
    )
    .expect("an offset");
    life_store.append(spec("permit", term, 1)).expect("appends");
    project_store
        .append(priced("alpha", 0.6, 1))
        .expect("appends");
    let stores = || {
        [
            (
                &life,
                &life_store as &dyn suzatary_store::store::Prodrome<TodoEvent>,
            ),
            (
                &project,
                &project_store as &dyn suzatary_store::store::Prodrome<TodoEvent>,
            ),
        ]
    };
    let first = Read::of_stores(stores());
    let before = row(&first.set(at(2)), "life", "permit");
    project_store
        .append(priced("alpha", 0.2, 3))
        .expect("appends");
    let second = first.moved(stores());
    assert!(second.shares(&first, &life.name), "life did not move");
    assert!(!second.shares(&first, &project.name), "the project did");
    let fresh = Read::of_stores(stores());
    for hour in [2, 4] {
        for _ in 0..2 {
            assert_eq!(
                row(&second.set(at(hour)), "life", "permit"),
                row(&fresh.set(at(hour)), "life", "permit"),
                "at {hour}h"
            );
        }
    }
    assert_eq!(row(&second.set(at(2)), "life", "permit"), before);
    let after = row(&second.set(at(4)), "life", "permit").expect("a number");
    assert!((after - 0.28).abs() < 1e-9, "{after}");
}

/// A chain whose policy alone moved is read again, as a fresh read under that policy reads
/// it: who is believed is part of what the reading is of.
#[test]
fn a_policy_that_moved_is_a_new_reading() {
    let store = InMemory::<TodoEvent>::default();
    store.append(priced("permit", 0.3, 1)).expect("appends");
    store
        .append(
            mk_spec_revised("permit", at(2), "triage", mk_flat(0.1).expect("a flat"), "")
                .expect("an event"),
        )
        .expect("appends");
    let triage = prodrome_core::event::Actor::new("triage").expect("an actor");
    let policy = |untrusted: Untrusted| suzatary_base::chains::ChainPolicy {
        name: name("life"),
        untrusted,
        roster: std::collections::BTreeSet::new(),
        root: suzatary_core::cast::keys::root(),
    };
    let (open, closed) = (policy(Untrusted::none()), policy(Untrusted::of([triage])));
    let of = |policy| {
        [(
            policy,
            &store as &dyn suzatary_store::store::Prodrome<TodoEvent>,
        )]
    };
    let first = Read::of_stores(of(&open));
    let second = first.moved(of(&closed));
    assert!(!second.shares(&first, &open.name), "the policy moved");
    let fresh = Read::of_stores(of(&closed));
    assert_eq!(
        through(&second.set(at(3)), "life", "permit"),
        through(&fresh.set(at(3)), "life", "permit")
    );
    assert_eq!(through(&second.set(at(3)), "life", "permit"), Some(0.3));
    assert_eq!(through(&first.set(at(3)), "life", "permit"), Some(0.1));
}

/// A row whose function refers into another chain is priced through the set, as a proposal
/// serving it is.
#[test]
fn a_row_reads_another_chain_through_the_set() {
    let term = fpl::mk_offset(
        0.1,
        mk_ref_in("suzatary".to_owned(), "alpha".to_owned()).expect("a ref"),
    )
    .expect("an offset");
    let chains = read(
        vec![
            ("life", legacy(vec![spec("permit", term, 1)])),
            ("suzatary", begun(vec![priced("alpha", 0.6, 1)])),
        ],
        &Untrusted::none(),
    );
    let set = chains.set(at(2));
    let value = row(&set, "life", "permit").expect("a number");
    assert!((value - 0.64).abs() < 1e-9, "{value}");
    assert_eq!(Some(value), through(&set, "life", "permit"));
}

// --- The clock, observed ----------------------------------------------------

fn hours_after(base: Datetime, hours: f64) -> Datetime {
    // Whole microseconds, the evaluator's grain.
    #[allow(clippy::cast_possible_truncation)]
    let us = (hours * 3_600_000_000.0) as i64;
    fpl::datetime_of(fpl::instant_of(base) + chrono::Duration::microseconds(us))
        .expect("an instant")
}

/// Every row of `chain` at `t`, as a whole percent shows it.
fn shown(read: &Read, chain: &str, t: Datetime) -> Specs<String, Option<u32>> {
    let set = read.set(t);
    set.entries(&name(chain))
        .expect("rows")
        .iter()
        .map(|row| {
            let value = row.value().expect("links");
            (
                row.key.as_str().to_owned(),
                Observation::PERCENT.observe(value),
            )
        })
        .collect()
}

fn next(read: &Read, chain: &str, t: Datetime) -> NextChange {
    let set = read.set(t);
    let rows = set.entries(&name(chain)).expect("rows");
    set.next(&name(chain), &rows, Observation::PERCENT)
        .expect("held")
}

/// One todo's price, by shape: a decay over its own window, the exact fragment, or one the
/// core answers conservatively, as the live chains hold them (a `Conj` of two, a `Recur`
/// of its own tendings, a `Within` that widens the span).
fn priced_as(todo: &str, shape: u8, lead: f64, start: f64, end: f64) -> Term {
    let decay = |lead: f64| {
        fpl::mk_decay(
            start,
            end,
            fpl::instant_of(hours_after(at(0), 2.0 * lead)),
            chrono::Duration::hours(1)
                + (fpl::instant_of(hours_after(at(0), lead)) - fpl::instant_of(at(0))),
            None,
        )
        .expect("a decay")
    };
    match shape {
        0 => decay(lead),
        1 => fpl::mk_conj(vec![decay(lead), decay(lead / 2.0)], -4.0).expect("a conj"),
        2 => fpl::mk_recur(
            todo.to_owned(),
            fpl::instant_of(at(0)),
            decay(lead),
            decay(lead),
        )
        .expect("a recur"),
        _ => fpl::mk_within(chrono::Duration::hours(6), -4.0, decay(lead)).expect("a within"),
    }
}

/// Todos priced over windows of their own, each read through a reference from the other
/// chain too, so a row priced through the set is scheduled as it is priced.
fn pricing(todos: &[(u8, f64, f64, f64)]) -> Read {
    let events = todos
        .iter()
        .enumerate()
        .map(|(i, &(shape, lead, start, end))| {
            let todo = format!("t{i}");
            spec(&todo, priced_as(&todo, shape, lead, start, end), 0)
        })
        .collect();
    let follows = mk_ref_in("life".to_owned(), "t0".to_owned()).expect("a reference");
    read(
        vec![
            ("life", legacy(events)),
            ("suzatary", begun(vec![spec("follows", follows, 0)])),
        ],
        &Untrusted::none(),
    )
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

    /// Between the instant a set is read at and its next change, every row of either chain
    /// reads the same whole percent, whatever the clock says; at an exact change a row
    /// reads another, and a conservative one is never later than the first that does.
    #[test]
    fn between_two_observed_changes_every_row_reads_the_same(
        todos in proptest::collection::vec((0u8..4, 1.0f64..48.0, 0.0f64..0.98, 0.0f64..0.9), 1..5),
        from in 0.0f64..120.0,
        within in 0.0f64..1.0,
    ) {
        let read = pricing(&todos);
        let t0 = hours_after(at(0), from);
        for chain in ["life", "suzatary"] {
            let before = shown(&read, chain, t0);
            let answer = next(&read, chain, t0);
            let Some(changes) = answer.at else {
                proptest::prop_assert!(answer.exact, "never is a proof");
                // Never again: a thousand days on reads the same.
                proptest::prop_assert_eq!(&shown(&read, chain, hours_after(t0, 24_000.0 * within)), &before);
                continue;
            };
            let span = (changes - fpl::instant_of(t0)).num_microseconds().expect("a span");
            proptest::prop_assert!(span > 0, "a change after the instant");
            #[allow(clippy::cast_precision_loss)]
            let t = hours_after(t0, within * (span - 1) as f64 / 3_600_000_000.0);
            proptest::prop_assert_eq!(&shown(&read, chain, t), &before);
            if answer.exact {
                let then = fpl::datetime_of(changes).expect("an instant");
                proptest::prop_assert_ne!(&shown(&read, chain, then), &before);
            }
        }
    }
}

/// An object stamped after the instant is not yet believed, so the reading moves at its
/// stamp even where no price moves with the clock.
#[test]
fn a_stamp_not_yet_believed_is_a_change() {
    let chains = read(
        vec![
            ("life", legacy(vec![priced("permit", 0.3, 1)])),
            (
                "suzatary",
                begun(vec![priced("alpha", 0.6, 1), priced("alpha", 0.4, 9)]),
            ),
        ],
        &Untrusted::none(),
    );
    assert_eq!(
        next(&chains, "life", at(2)),
        NextChange {
            at: Some(fpl::instant_of(at(9))),
            exact: true
        }
    );
    assert_eq!(
        next(&chains, "life", at(10)).at,
        None,
        "flat, and every stamp believed"
    );
}
