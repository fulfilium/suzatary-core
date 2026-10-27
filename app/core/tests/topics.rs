//! The topic stores' laws, over real replicas of the Prodrome's in-memory store: each device writes
//! what its own reading plans, devices sync by the Prodrome's `sync` (the union of two object
//! sets), and every law is about the reading after that union.
//!
//! The merge laws of the stores are the Prodrome's (`prodrome_core::tags`, SPEC law 48). Here:
//! - The box's door merges as the union does: a box that takes each device's own objects through
//!   `sync::receive_changes`, whatever order they are posted in, reads what a device holding the
//!   union of every device's objects reads.
//! - The stream's filter shows exactly the targets that carry every chosen topic.
//! - Every event, topic and target prints and parses back to itself.

use std::collections::BTreeSet;

use prodrome_core::event::{
    Actor, Envelope, canonical, canonical_envelope, parse_event, seal_hash,
};
use prodrome_core::genesis::mk_genesis;
use prodrome_core::literal::Datetime;
use prodrome_core::schema::Schema;
use prodrome_core::store::{MemoryStore, Replica, sync};
use proptest::prelude::*;

use suzatary_core::secretary::store::ConversationId;
use suzatary_core::secretary::topics::reading::{
    AliasEvent, AliasVocabulary, MembershipEvent, MembershipVocabulary, Reading, Topics,
};
use suzatary_core::secretary::topics::target::Target;
use suzatary_core::secretary::topics::topic::Topic;

const NAMES: [&str; 4] = ["travel", "trips", "work", "job"];

fn topic(name: &str) -> Topic {
    Topic::new(name).expect("a topic")
}

fn target(n: usize) -> Target {
    let conversation = ConversationId::new("default").expect("a slug");
    if n % 3 == 2 {
        Target::Note {
            conversation,
            turn: n,
        }
    } else {
        Target::Round {
            conversation,
            turn: n,
        }
    }
}

fn at(minute: u32) -> Datetime {
    Datetime::new(2026, 10, 21, 8 + (minute / 60) % 12, minute % 60, 0, 0).expect("an instant")
}

/// A store at schema `E` holding one genesis, the same on every device.
fn begun<E: Schema<Vocabulary: Default>>() -> MemoryStore<E> {
    let store = MemoryStore::default();
    let genesis: Envelope<E> =
        Envelope::Genesis(mk_genesis("topics", &"0".repeat(32)).expect("a genesis"));
    let print = canonical_envelope(&genesis).into_bytes();
    store
        .receive([seal_hash(&genesis)].into(), &|_| Some(print.clone()))
        .expect("begins");
    store
}

/// The names of the objects `store` holds.
fn names<E: Schema>(store: &MemoryStore<E>) -> BTreeSet<String> {
    let held = store.held().expect("held");
    held.dag
        .objects()
        .keys()
        .map(|name| name.as_str().to_owned())
        .collect()
}

/// One device: its replica of each store, and its name as a writer.
struct Device {
    memberships: MemoryStore<MembershipEvent>,
    aliases: MemoryStore<AliasEvent>,
    actor: Actor,
    clock: u32,
}

impl Device {
    fn new(n: usize) -> Self {
        Self {
            memberships: begun(),
            aliases: begun(),
            actor: Actor::new(format!("device{n}")).expect("an actor"),
            clock: 0,
        }
    }

    fn read(&self) -> Reading {
        let memberships = self
            .memberships
            .held()
            .expect("held")
            .dag
            .nodes()
            .expect("nodes");
        let aliases = self
            .aliases
            .held()
            .expect("held")
            .dag
            .nodes()
            .expect("nodes");
        suzatary_core::secretary::topics::reading::of(&memberships, &aliases).expect("one genesis")
    }

    fn objects(&self) -> (BTreeSet<String>, BTreeSet<String>) {
        (names(&self.memberships), names(&self.aliases))
    }

    /// Take in everything `other` holds.
    fn take(&self, other: &Device) {
        sync(&other.memberships, &self.memberships).expect("synced");
        sync(&other.aliases, &self.aliases).expect("synced");
    }

    fn tick(&mut self) -> Datetime {
        self.clock += 1;
        at(self.clock)
    }

    /// Write what this device's reading plans for `op`; answers how many objects it wrote.
    fn apply(&mut self, op: &Op) -> usize {
        let reading = self.read();
        let at = self.tick();
        match op {
            Op::Add(n, name) => self.write_memberships(
                reading
                    .add(&target(*n), &topic(NAMES[*name]), at, &self.actor)
                    .into_iter()
                    .collect(),
            ),
            Op::Remove(n, name) => self.write_memberships(reading.remove(
                &target(*n),
                &topic(NAMES[*name]),
                at,
                &self.actor,
            )),
            Op::Rename(from, to) => {
                let events =
                    reading.rename(&topic(NAMES[*from]), &topic(NAMES[*to]), at, &self.actor);
                let written = events.len();
                for event in events {
                    self.aliases.append(event).expect("appended");
                }
                written
            }
        }
    }

    fn write_memberships(&self, events: Vec<MembershipEvent>) -> usize {
        let written = events.len();
        for event in events {
            self.memberships.append(event).expect("appended");
        }
        written
    }
}

#[derive(Debug, Clone)]
enum Op {
    Add(usize, usize),
    Remove(usize, usize),
    Rename(usize, usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (0usize..4, 0usize..4).prop_map(|(n, name)| Op::Add(n, name)),
        2 => (0usize..4, 0usize..4).prop_map(|(n, name)| Op::Remove(n, name)),
        1 => (0usize..4, 0usize..4).prop_map(|(from, to)| Op::Rename(from, to)),
    ]
}

/// A step of a script: a device writes, or a device takes in another's objects.
#[derive(Debug, Clone)]
enum Step {
    Write(usize, Op),
    Sync { from: usize, to: usize },
}

fn steps(devices: usize) -> impl Strategy<Value = Vec<Step>> {
    proptest::collection::vec(
        prop_oneof![
            3 => (0..devices, op()).prop_map(|(device, op)| Step::Write(device, op)),
            1 => (0..devices, 0..devices).prop_map(|(from, to)| Step::Sync { from, to }),
        ],
        0..30,
    )
}

fn run(devices: &mut [Device], script: &[Step]) {
    for step in script {
        match step {
            Step::Write(device, op) => {
                devices[*device].apply(op);
            }
            Step::Sync { from, to } => {
                if from != to {
                    let (from, to) = (&devices[*from], &devices[*to]);
                    to.take(from);
                }
            }
        }
    }
}

fn devices(n: usize) -> Vec<Device> {
    (0..n).map(Device::new).collect()
}

/// A fresh device holding the union of `parts`, taken in that order.
fn union(parts: &[&Device]) -> Device {
    let merged = Device::new(9);
    for part in parts {
        merged.take(part);
    }
    merged
}

fn topics_of(reading: &Reading, n: usize) -> BTreeSet<Topic> {
    reading.tags(&target(n))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Adding twice writes once, and removing twice writes once.
    #[test]
    fn a_second_run_writes_nothing(script in proptest::collection::vec(op(), 0..20), n in 0usize..4, name in 0usize..4) {
        let mut device = Device::new(0);
        for op in &script {
            device.apply(op);
        }
        device.apply(&Op::Add(n, name));
        prop_assert_eq!(device.apply(&Op::Add(n, name)), 0);
        prop_assert!(device.apply(&Op::Remove(n, name)) > 0);
        prop_assert_eq!(device.apply(&Op::Remove(n, name)), 0);
        prop_assert!(!topics_of(&device.read(), n).contains(&device.read().spelling(&topic(NAMES[name]))));
    }

    /// Devices write apart and sync among themselves; then each posts to the box the objects
    /// it wrote, in any order, and posts again what was refused for a parent not yet there.
    /// The box takes every object and reads what the union reads, and a second post of
    /// everything takes nothing.
    #[test]
    fn the_box_door_merges_as_the_union_does(script in steps(3), order in Just(()).prop_perturb(|(), mut rng| {
        let mut order = vec![0usize, 1, 2];
        for i in (1..order.len()).rev() {
            order.swap(i, rng.random_range(0..=i));
        }
        order
    })) {
        let mut all = devices(3);
        run(&mut all, &script);
        let the_box = Device::new(8);
        let mut rounds = 0;
        loop {
            let mut took = 0;
            for &device in &order {
                took += post(&all[device], &the_box);
            }
            rounds += 1;
            if took == 0 || rounds > 40 {
                break;
            }
        }
        let union = union(&[&all[0], &all[1], &all[2]]);
        prop_assert_eq!(the_box.objects(), union.objects());
        prop_assert_eq!(the_box.read(), union.read());
        for device in &all {
            prop_assert_eq!(post(device, &the_box), 0, "a re-post takes nothing");
        }
    }

    /// Every event a device writes prints and parses back to itself.
    #[test]
    fn every_event_prints_and_parses_back(script in proptest::collection::vec(op(), 0..20)) {
        let mut device = Device::new(0);
        for op in &script {
            device.apply(op);
        }
        for envelope in device.memberships.held().expect("held").dag.objects().values() {
            if let Some(event) = envelope.event() {
                let print = canonical(event);
                prop_assert_eq!(&parse_event::<MembershipEvent>(&MembershipVocabulary::default(), &print).expect("parses"), event);
            }
        }
        for envelope in device.aliases.held().expect("held").dag.objects().values() {
            if let Some(event) = envelope.event() {
                let print = canonical(event);
                prop_assert_eq!(&parse_event::<AliasEvent>(&AliasVocabulary::default(), &print).expect("parses"), event);
            }
        }
    }

    /// The stream's filter shows exactly the targets carrying every chosen topic, whatever was
    /// added, taken off and renamed on two devices: no topic chosen shows every target, a
    /// combination shows what each of its topics shows, and a name filters as its topic does.
    #[test]
    fn a_filter_shows_exactly_the_rounds_holding_every_chosen_topic(
        script in steps(2),
        chosen in proptest::collection::btree_set(0usize..4, 0..=4),
    ) {
        let mut devices = devices(2);
        run(&mut devices, &script);
        let reading = union(&[&devices[0], &devices[1]]).read();
        let chosen: BTreeSet<Topic> = chosen.iter().map(|n| topic(NAMES[*n])).collect();
        let one = |name: &Topic| BTreeSet::from([name.clone()]);
        for n in 0..6 {
            let at = target(n);
            let on = topics_of(&reading, n);
            prop_assert!(reading.shows(&BTreeSet::new(), &at));
            prop_assert_eq!(
                reading.shows(&chosen, &at),
                chosen.iter().all(|name| on.contains(&reading.spelling(name)))
            );
            prop_assert_eq!(
                reading.shows(&chosen, &at),
                chosen.iter().all(|name| reading.shows(&one(name), &at))
            );
            for name in &chosen {
                for alias in reading.names(name) {
                    prop_assert_eq!(reading.shows(&one(&alias), &at), reading.shows(&one(name), &at));
                }
            }
        }
    }

    /// A target prints and parses back to itself.
    #[test]
    fn a_target_prints_and_parses_back(n in any::<usize>(), conversation in "[a-z0-9][a-z0-9-]{0,20}") {
        let conversation = ConversationId::new(conversation).expect("a slug");
        for target in [
            Target::Round { conversation: conversation.clone(), turn: n },
            Target::Note { conversation: conversation.clone(), turn: n },
        ] {
            prop_assert_eq!(target.to_string().parse::<Target>(), Ok(target.clone()));
        }
    }
}

/// What the box takes when `device` posts every object it holds of its own actor's, in each
/// store: how many objects it took.
fn post(device: &Device, the_box: &Device) -> usize {
    posted(&device.memberships, &the_box.memberships, &device.actor)
        + posted(&device.aliases, &the_box.aliases, &device.actor)
}

fn posted<E: Schema<Vocabulary: Default>>(
    from: &MemoryStore<E>,
    to: &MemoryStore<E>,
    actor: &Actor,
) -> usize {
    let mine: Vec<String> = from
        .held()
        .expect("held")
        .dag
        .objects()
        .values()
        .filter(|object| object.event().is_some_and(|event| event.actor() == actor))
        .map(canonical_envelope)
        .collect();
    let held: BTreeSet<prodrome_core::event::Hash> = to
        .held()
        .expect("held")
        .dag
        .objects()
        .keys()
        .cloned()
        .collect();
    let outcome = suzatary_core::sync::receive_changes::<E>(&held, &mine, actor);
    let prints: std::collections::BTreeMap<_, _> = outcome
        .taken
        .iter()
        .map(|(name, print)| (name.clone(), print.clone().into_bytes()))
        .collect();
    to.receive(prints.keys().cloned().collect(), &|name| {
        prints.get(name).cloned()
    })
    .expect("the box takes what its door took");
    outcome.taken.len()
}

/// The box takes a tab's topic only as a change written as the poster, resting on what it holds:
/// a change naming another actor, a prodrome, a signature, a legacy object, bytes that are not
/// one canonical object and a change resting on an unheld object are each refused, and an
/// object it holds is answered as held whoever posts it.
#[test]
fn the_box_door_takes_only_the_posters_changes() {
    use prodrome_core::change::mk_change;
    use suzatary_core::sync::{Reason, receive_changes};
    let ada = Actor::new("ada").expect("an actor");
    let genesis: Envelope<MembershipEvent> =
        Envelope::Genesis(mk_genesis("topics", &"0".repeat(32)).expect("a genesis"));
    let begun = seal_hash(&genesis);
    let held: BTreeSet<_> = [begun.clone()].into();
    let change = |actor: &Actor, deps: Vec<prodrome_core::event::Hash>| {
        Envelope::Change(
            mk_change(
                begun.clone(),
                deps,
                prodrome_core::tags::membership::added::<Topics>(
                    target(0),
                    topic("travel"),
                    at(1),
                    actor.clone(),
                ),
            )
            .expect("a change"),
        )
    };
    let mine = change(&ada, Vec::new());
    let unheld = prodrome_core::event::Hash::new("f".repeat(64)).expect("a name");
    let child = Envelope::Change(
        mk_change(
            begun.clone(),
            vec![seal_hash(&mine)],
            prodrome_core::tags::membership::removed::<Topics>(
                target(0),
                topic("travel"),
                at(2),
                ada.clone(),
            ),
        )
        .expect("a change"),
    );
    let refused = |print: String| {
        let outcome = receive_changes::<MembershipEvent>(&held, &[print], &ada);
        assert!(outcome.taken.is_empty() && outcome.already.is_empty());
        outcome
            .refused
            .into_iter()
            .map(|r| r.reason)
            .collect::<Vec<_>>()
    };
    let theirs = change(&Actor::new("triage").expect("an actor"), Vec::new());
    assert!(matches!(
        refused(canonical_envelope(&theirs)).as_slice(),
        [Reason::ForeignActor { .. }]
    ));
    let other: Envelope<MembershipEvent> =
        Envelope::Genesis(mk_genesis("topics", &"1".repeat(32)).expect("a genesis"));
    assert_eq!(refused(canonical_envelope(&other)), vec![Reason::BoxOnly]);
    let legacy: Envelope<MembershipEvent> = prodrome_core::event::mk_sealed(
        None,
        prodrome_core::tags::membership::added::<Topics>(
            target(0),
            topic("travel"),
            at(1),
            ada.clone(),
        ),
    );
    assert_eq!(
        refused(canonical_envelope(&legacy)),
        vec![Reason::NotAChange("Sealed")]
    );
    assert!(matches!(
        refused(format!("{} ", canonical_envelope(&mine))).as_slice(),
        [Reason::NotAnObject(_)]
    ));
    let orphan = change(&ada, vec![unheld.clone()]);
    assert_eq!(
        refused(canonical_envelope(&orphan)),
        vec![Reason::MissingParent(unheld)]
    );
    // A child posted before its parent is taken with it, parents first.
    let both = receive_changes::<MembershipEvent>(
        &held,
        &[canonical_envelope(&child), canonical_envelope(&mine)],
        &ada,
    );
    let names: Vec<_> = both.taken.iter().map(|(name, _)| name.clone()).collect();
    assert_eq!(names, vec![seal_hash(&mine), seal_hash(&child)]);
    let again = receive_changes::<MembershipEvent>(
        &[begun.clone(), seal_hash(&theirs)].into(),
        &[canonical_envelope(&theirs)],
        &ada,
    );
    assert_eq!(again.already, vec![seal_hash(&theirs)]);
}
