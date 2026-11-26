//! The decision schema's laws (`docs/design-register-types.md` §7 in the Prodrome): the life is
//! a partial order (law 2); the reading is a function of the object set, whatever order the
//! objects arrive in (law 1), and what has gone out of force stays out; the store's own append
//! refuses a restatement of a withdrawn decision (law 4); two replicas that adopt each other read
//! one ledger at or above both; supersession is a partial order on a ledger no writer here closed
//! a cycle in, and the lineages are a linear extension of it; and every event prints and parses
//! back.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_dag::dag::Dag;
use prodrome_dag::registers::Node;
use prodrome_data::event::{Actor, Envelope, Hash, canonical, canonical_envelope, parse_event};
use prodrome_data::fold::Order;
use prodrome_data::literal::Datetime;
use prodrome_data::policy::Everything;
use prodrome_store::store::EventStore;
use proptest::prelude::*;

use suzatary_record::record::MarkupSource;
use suzatary_store::store::linearised;
use suzatary_todo::decisions::{
    self, Area, DecisionEvent, DecisionId, DecisionVocabulary, Ledger, Life, Statement, Summaries,
};

type Disk = EventStore<DecisionEvent, Everything>;

/// A store at the decision schema, removed when dropped.
struct Scratch(
    Disk,
    #[expect(dead_code, reason = "held for its drop, which removes the directory")]
    tempfile::TempDir,
);

impl Scratch {
    fn new() -> Self {
        let scratch = Self::empty();
        scratch.0.init("decisions").expect("begins");
        scratch
    }

    /// A store with nothing in it, not even a genesis: a replica about to adopt one.
    fn empty() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("suzatary-decisions-schema-")
            .tempdir()
            .expect("a scratch directory");
        Self(Disk::new(dir.path().join("store"), Everything), dir)
    }
}

fn at(minute: usize) -> Datetime {
    let minute = u32::try_from(minute % 600).expect("a minute");
    Datetime::new(2026, 10, 3, 8 + minute / 60, minute % 60, 0, 0).expect("an instant")
}

fn id(n: usize) -> DecisionId {
    DecisionId::new(format!("decision{n}")).expect("an id")
}

fn statement(seed: u64) -> Statement {
    Statement::new(
        &format!("Decision {seed}"),
        MarkupSource::escaped(&format!("It says *{seed}*.")),
        &format!("Because {}.", seed % 7),
    )
    .expect("a statement")
}

fn ada() -> Actor {
    Actor::new("ada").expect("an actor")
}

fn area(part: usize) -> Area {
    Area::new(format!("app/part{part}")).expect("an area")
}

/// A node of the decision log, by a name the tests choose.
fn node_of(n: usize) -> Hash {
    Hash::of_bytes(format!("node {n}").as_bytes())
}

/// What a writer does: decide a new decision linking to earlier ones, revise a standing one
/// adding links to earlier ones, or withdraw a standing one.
#[derive(Debug, Clone)]
enum Step {
    Decide(u64, Vec<usize>),
    Revise(usize, u64, Vec<usize>),
    Withdraw(usize),
    Govern(usize, Vec<usize>),
    Summarize(usize, u64),
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    let links = || proptest::collection::vec(0usize..40, 0..3);
    proptest::collection::vec(
        prop_oneof![
            3 => (any::<u64>(), links()).prop_map(|(seed, links)| Step::Decide(seed, links)),
            2 => (0usize..40, any::<u64>(), links())
                .prop_map(|(pick, seed, links)| Step::Revise(pick, seed, links)),
            1 => (0usize..40).prop_map(Step::Withdraw),
            1 => (0usize..40, proptest::collection::vec(0usize..5, 1..3))
                .prop_map(|(pick, areas)| Step::Govern(pick, areas)),
            1 => (0usize..6, any::<u64>()).prop_map(|(node, seed)| Step::Summarize(node, seed)),
        ],
        1..30,
    )
}

/// The events `script` makes, as the store's writer would allow them: a link only from a later
/// decision to an earlier one, so no cycle; no restatement of a withdrawn decision.
fn history(script: &[Step], from: usize) -> Vec<DecisionEvent> {
    let mut decided = 0usize;
    let mut withdrawn: BTreeSet<usize> = BTreeSet::new();
    let mut events = Vec::new();
    let earlier = |of: usize, picks: &[usize]| -> BTreeSet<DecisionId> {
        picks
            .iter()
            .filter(|_| of > 0)
            .map(|pick| id(pick % of))
            .collect()
    };
    for (n, step) in script.iter().enumerate() {
        let now = at(from + n);
        match step {
            Step::Decide(seed, links) => {
                let links = earlier(decided, links);
                events.push(
                    decisions::decided(id(decided), now, ada(), statement(*seed), links)
                        .expect("an event"),
                );
                decided += 1;
            }
            Step::Revise(pick, seed, links) if decided > 0 => {
                let which = pick % decided;
                if !withdrawn.contains(&which) {
                    let links = earlier(which, links);
                    events.push(
                        decisions::revised(id(which), now, ada(), statement(*seed), links)
                            .expect("an event"),
                    );
                }
            }
            Step::Withdraw(pick) if decided > 0 => {
                let which = pick % decided;
                if withdrawn.insert(which) {
                    events.push(
                        decisions::withdrawn(id(which), now, ada(), "No longer held.")
                            .expect("an event"),
                    );
                }
            }
            Step::Govern(pick, areas) if decided > 0 => {
                let which = pick % decided;
                if !withdrawn.contains(&which) {
                    let areas = areas.iter().map(|part| area(*part)).collect();
                    events
                        .push(decisions::governs(id(which), now, ada(), areas).expect("an event"));
                }
            }
            Step::Summarize(node, seed) => events.push(
                decisions::summarized(&node_of(*node), now, ada(), &format!("Summary {seed}"))
                    .expect("an event"),
            ),
            Step::Revise(..) | Step::Withdraw(_) | Step::Govern(..) => {}
        }
    }
    events
}

fn written(events: &[DecisionEvent]) -> Scratch {
    let scratch = Scratch::new();
    for event in events {
        scratch.0.append(event.clone()).expect("allowed");
    }
    scratch
}

fn objects(store: &Disk) -> Vec<(Hash, Envelope<DecisionEvent>)> {
    linearised(store).expect("linearised")
}

fn read(objects: Vec<(Hash, Envelope<DecisionEvent>)>) -> Ledger {
    let nodes: Vec<Node<DecisionEvent>> = Dag::from_iter(objects).nodes().expect("a prodrome");
    Ledger::of(&nodes)
}

fn summaries(objects: Vec<(Hash, Envelope<DecisionEvent>)>) -> Summaries {
    let nodes: Vec<Node<DecisionEvent>> = Dag::from_iter(objects).nodes().expect("a prodrome");
    Summaries::of(&nodes)
}

/// `objects` in an order a replica could receive them, every parent before its child.
fn arrival(
    objects: &[(Hash, Envelope<DecisionEvent>)],
    mut seed: u64,
) -> Vec<(Hash, Envelope<DecisionEvent>)> {
    let mut waiting = objects.to_vec();
    let mut arrived: Vec<(Hash, Envelope<DecisionEvent>)> = Vec::new();
    while !waiting.is_empty() {
        let ready: Vec<usize> = (0..waiting.len())
            .filter(|&at| {
                prodrome_data::event::parents_of(&waiting[at].1)
                    .iter()
                    .all(|parent| arrived.iter().any(|(held, _)| held == parent))
            })
            .collect();
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let pick = ready[usize::try_from(seed >> 33).expect("fits") % ready.len()];
        arrived.push(waiting.remove(pick));
    }
    arrived
}

fn adopt(to: &Disk, from: &Disk) {
    let prints: BTreeMap<Hash, String> = objects(from)
        .iter()
        .map(|(name, object)| (name.clone(), canonical_envelope(object)))
        .collect();
    for tip in from.tips().expect("tips") {
        to.adopt_objects(&prints, &tip, prodrome_data::husk::Sender::Stranger)
            .expect("adopted");
    }
}

fn out_of_force(ledger: &Ledger) -> BTreeSet<DecisionId> {
    ledger
        .iter()
        .map(|(id, _)| id.clone())
        .filter(|id| !ledger.in_force(id))
        .collect()
}

#[test]
fn a_life_is_a_partial_order_with_withdrawn_the_top() {
    let all = [Life::Stated, Life::Withdrawn];
    suzatary_store::laws::partial_order(&all).expect("a partial order");
    assert!(all.iter().all(|a| a.le(&Life::Withdrawn)));
    assert!(!Life::Withdrawn.le(&Life::Stated));
}

#[test]
fn two_concurrent_revisions_are_a_conflict_shown_whole() {
    let one = written(&history(&[Step::Decide(1, Vec::new())], 0));
    let two = Scratch::empty();
    adopt(&two.0, &one.0);
    one.0
        .append(
            decisions::revised(id(0), at(10), ada(), statement(2), BTreeSet::new())
                .expect("an event"),
        )
        .expect("allowed");
    two.0
        .append(
            decisions::revised(id(0), at(11), ada(), statement(3), BTreeSet::new())
                .expect("an event"),
        )
        .expect("allowed");
    adopt(&one.0, &two.0);
    let ledger = read(objects(&one.0));
    let decision = ledger.get(&id(0)).expect("decided");
    assert!(decision.is_conflict());
    let titles: BTreeSet<&str> = decision.statements().iter().map(Statement::title).collect();
    assert_eq!(titles, BTreeSet::from(["Decision 2", "Decision 3"]));
}

#[test]
fn concurrent_links_that_close_a_cycle_are_named_and_every_decision_is_shown_once() {
    let one = written(&history(
        &[Step::Decide(1, Vec::new()), Step::Decide(2, Vec::new())],
        0,
    ));
    let two = Scratch::empty();
    adopt(&two.0, &one.0);
    one.0
        .append(
            decisions::revised(id(0), at(10), ada(), statement(1), BTreeSet::from([id(1)]))
                .expect("an event"),
        )
        .expect("allowed");
    two.0
        .append(
            decisions::revised(id(1), at(11), ada(), statement(2), BTreeSet::from([id(0)]))
                .expect("an event"),
        )
        .expect("allowed");
    adopt(&one.0, &two.0);
    let ledger = read(objects(&one.0));
    assert_eq!(ledger.cycles(), BTreeSet::from([&id(0), &id(1)]));
    let shown: Vec<&DecisionId> = ledger.lineages().concat();
    assert_eq!(shown.len(), 2);
    assert!(!ledger.in_force(&id(0)) && !ledger.in_force(&id(1)));
}

proptest! {
    /// Every event prints and parses back to itself.
    #[test]
    fn every_event_prints_and_parses_back(script in steps()) {
        for event in history(&script, 0) {
            let print = canonical(&event);
            prop_assert_eq!(parse_event::<DecisionEvent>(&DecisionVocabulary::default(), &print).expect("parses"), event);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// The reading is a function of the object set: every order a replica could receive the
    /// objects in reads the same, every prefix of that arrival holds at most the whole, and a
    /// decision gone out of force in a prefix is out of force in the whole; what the prefixes of
    /// two arrivals hold is a partial order.
    #[test]
    fn the_reading_is_the_object_sets_whatever_order_they_arrive_in(
        script in steps(),
        seed in any::<u64>(),
        other in any::<u64>(),
    ) {
        let store = written(&history(&script, 0));
        let all = objects(&store.0);
        let whole = read(all.clone());
        let arrived = arrival(&all, seed);
        prop_assert_eq!(&read(arrived.clone()), &whole);
        prop_assert_eq!(summaries(arrived.clone()), summaries(all.clone()));
        let gone = out_of_force(&whole);
        let mut held = Vec::new();
        for end in 1..=arrived.len() {
            let prefix = read(arrived[..end].to_vec());
            prop_assert!(prefix.held().le(&whole.held()));
            prop_assert!(out_of_force(&prefix).is_subset(&gone), "a decision stood again");
            held.push(prefix.held());
        }
        // A second arrival's prefixes, so pairs no one arrival orders are compared too.
        let second = arrival(&all, other);
        held.extend((1..=second.len()).map(|end| read(second[..end].to_vec()).held()));
        suzatary_store::laws::partial_order(&held)?;
        prop_assert!(store.0.verify().is_empty(), "{:?}", store.0.verify());
    }

    /// Once withdrawn, always withdrawn: the store refuses any restatement of a withdrawn
    /// decision and writes nothing.
    #[test]
    fn the_store_refuses_to_restate_a_withdrawn_decision(script in steps(), seed in any::<u64>()) {
        let store = written(&history(&script, 0));
        let before = objects(&store.0).len();
        let reading = read(objects(&store.0));
        for (withdrawn, _) in reading.iter().filter(|(_, decision)| decision.withdrawn.is_some()) {
            let again = decisions::revised(withdrawn.clone(), at(500), ada(), statement(seed), BTreeSet::new())
                .expect("an event");
            prop_assert!(store.0.append(again).is_err());
            let anew = decisions::decided(withdrawn.clone(), at(501), ada(), statement(seed), BTreeSet::new())
                .expect("an event");
            prop_assert!(store.0.append(anew).is_err());
        }
        prop_assert_eq!(objects(&store.0).len(), before);
    }

    /// Two replicas share a prefix and each writes on; adopting each other's objects, both read
    /// one ledger, at or above each one's own, and the union of histories is the join of what
    /// each holds.
    #[test]
    fn the_union_reads_at_or_above_both(
        shared in steps(),
        mine in steps(),
        theirs in steps(),
    ) {
        let prefix = history(&shared, 0);
        let one = written(&prefix);
        let base = objects(&one.0);
        let two = Scratch::empty();
        adopt(&two.0, &one.0);
        // Each side's own decisions under ids the other does not use, and its own withdrawals.
        for (store, script, offset) in [(&one, &mine, 100), (&two, &theirs, 200)] {
            for event in history(script, offset) {
                let renamed = rename(&event, offset);
                let _ = store.0.append(renamed);
            }
        }
        let (a, b) = (read(objects(&one.0)), read(objects(&two.0)));
        // The ledger's join is the union of histories: what it holds is their least upper bound.
        let union = |mut x: Vec<(Hash, Envelope<DecisionEvent>)>, y: Vec<(Hash, Envelope<DecisionEvent>)>| {
            for object in y {
                if !x.iter().any(|(name, _)| *name == object.0) {
                    x.push(object);
                }
            }
            x
        };
        suzatary_store::laws::join(
            &[base, objects(&one.0), objects(&two.0)],
            |history| read(history.clone()).held(),
            union,
        )?;
        adopt(&one.0, &two.0);
        adopt(&two.0, &one.0);
        let (after_one, after_two) = (read(objects(&one.0)), read(objects(&two.0)));
        prop_assert_eq!(&after_one, &after_two);
        prop_assert!(a.held().le(&after_one.held()) && b.held().le(&after_one.held()));
    }

    /// On a ledger with no cycle, supersession is reflexive, antisymmetric and transitive, and
    /// the lineages list every decision once, each before everything it supersedes.
    #[test]
    fn supersession_is_a_partial_order_and_the_lineages_extend_it(script in steps()) {
        let ledger = read(objects(&written(&history(&script, 0)).0));
        prop_assert!(ledger.cycles().is_empty());
        let ids: Vec<&DecisionId> = ledger.iter().map(|(id, _)| id).collect();
        for a in &ids {
            prop_assert!(ledger.below(a, a));
            for b in &ids {
                if ledger.below(a, b) && ledger.below(b, a) {
                    prop_assert_eq!(a, b);
                }
                for c in &ids {
                    if ledger.below(a, b) && ledger.below(b, c) {
                        prop_assert!(ledger.below(a, c));
                    }
                }
            }
        }
        let shown: Vec<&DecisionId> = ledger.lineages().concat();
        prop_assert_eq!(shown.len(), ids.len());
        let place: BTreeMap<&DecisionId, usize> =
            shown.iter().enumerate().map(|(at, id)| (*id, at)).collect();
        prop_assert_eq!(place.len(), ids.len());
        for (id, decision) in ledger.iter() {
            for below in &decision.supersedes {
                if let Some(&under) = place.get(below) {
                    prop_assert!(place[id] < under, "{id} is shown after {below}");
                }
            }
        }
    }
}

/// `event` about `decisionN + offset`, linking likewise: a writer's own ids, no other's.
fn rename(event: &DecisionEvent, offset: usize) -> DecisionEvent {
    let moved = |of: &DecisionId| {
        let n: usize = of.as_str()["decision".len()..].parse().expect("a number");
        id(n + offset)
    };
    let decision = || moved(prodrome_data::schema::Schema::key(event));
    let when = prodrome_data::schema::Schema::at(event);
    match event.body() {
        suzatary_todo::decisions::Said::Decided {
            statement,
            supersedes,
        } => decisions::decided(
            decision(),
            when,
            ada(),
            statement.clone(),
            supersedes.iter().map(moved).collect(),
        ),
        suzatary_todo::decisions::Said::Revised {
            statement,
            supersedes,
        } => decisions::revised(
            decision(),
            when,
            ada(),
            statement.clone(),
            supersedes.iter().map(moved).collect(),
        ),
        suzatary_todo::decisions::Said::Withdrawn { reason } => {
            decisions::withdrawn(decision(), when, ada(), reason.as_str())
        }
        suzatary_todo::decisions::Said::Governs { areas } => {
            decisions::governs(decision(), when, ada(), areas.clone())
        }
        // A summary names a node of the log, which no replica renames.
        suzatary_todo::decisions::Said::Summarized { .. } => Ok(event.clone()),
    }
    .expect("an event")
}

/// Every constructor the decisions store and the inbox's decision card print, with its fields
/// in print order.
#[test]
fn the_stored_names_are_pinned() {
    use prodrome_data::schema::headed::Body;
    use suzatary_todo::decisions::{DECIDE, Said};
    let stored = [<Said as Body>::EVENTS, &[DECIDE]].concat();
    prodrome_data::literal::codec::assert_pinned(
        &stored,
        "
        Decided(decision, at, actor, title, body, rationale, supersedes)
        Revised(decision, at, actor, title, body, rationale, supersedes)
        Withdrawn(decision, at, actor, reason)
        Governs(decision, at, actor, areas)
        Summarized(decision, at, actor, words)
        Decide(id, title, body, rationale, supersedes)
        ",
    );
}

/// A summary is no decision, and an area never changes what a decision says: both only add to the
/// reading of a node or of the areas.
#[test]
fn a_summary_and_an_area_add_to_the_reading_and_decide_nothing() {
    let store = written(&[
        decisions::decided(id(1), at(1), ada(), statement(1), BTreeSet::new()).expect("an event"),
    ]);
    let before = read(objects(&store.0));
    for event in [
        decisions::governs(id(1), at(2), ada(), BTreeSet::from([area(1)])).expect("an event"),
        decisions::governs(id(1), at(3), ada(), BTreeSet::from([area(2)])).expect("an event"),
        decisions::summarized(&node_of(1), at(4), ada(), "A line.").expect("an event"),
    ] {
        store.0.append(event).expect("allowed");
    }
    let all = objects(&store.0);
    let after = read(all.clone());
    assert_eq!(after.len(), before.len());
    let decision = after.get(&id(1)).expect("decided");
    assert_eq!(
        decision.statement(),
        before.get(&id(1)).expect("decided").statement()
    );
    assert_eq!(decision.areas, BTreeSet::from([area(1), area(2)]));
    assert_eq!(summaries(all.clone()).get(&node_of(1)), Some("A line."));
    assert_eq!(summaries(all).get(&node_of(2)), None);
}
