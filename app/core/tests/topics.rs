//! The topic stores' laws, over real replicas of the Prodrome's in-memory store: each device writes
//! what its own reading plans, devices sync by the Prodrome's `sync` (the union of two object
//! sets), and every law is about the reading after that union.
//!
//! The merge laws of the stores are the Prodrome's (`prodrome_core::tags`, SPEC law 48). Here:
//! - The box's door merges as the union does: a box that takes each device's own signed objects
//!   through `sync::receive_changes`, whatever order they are posted in, reads what a device
//!   holding the union of every device's objects reads.
//! - The door takes a change only with a signature every chain trusts for its writer, and
//!   records a refusal only where such a signature proves the refused change.
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

use suzatary_base::conversation_id::ConversationId;
use suzatary_base::topic_target::Target;
use suzatary_core::secretary::topics::provenance::Owner;
use suzatary_core::secretary::topics::reading::{
    AliasEvent, AliasVocabulary, MembershipEvent, MembershipVocabulary, Reading, Topics,
};
use suzatary_core::secretary::topics::standing::Standing;
use suzatary_core::secretary::topics::topic::Topic;
use suzatary_signatures::wire::signatures::RefusalKind;

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
    actor: Owner,
    clock: u32,
}

impl Device {
    fn new(n: usize) -> Self {
        Self {
            memberships: begun(),
            aliases: begun(),
            actor: Owner::new(Actor::new(format!("device{n}")).expect("an actor"))
                .expect("not the secretary"),
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
            Op::Suggest(n, name) => self.write_memberships(
                reading
                    .suggestion(&target(*n), &topic(NAMES[*name]), at)
                    .into_iter()
                    .collect(),
            ),
            Op::Drop(n, name) => match reading.standing(&target(*n), &topic(NAMES[*name])) {
                Standing::Suggested(suggestion) => self.write_memberships(suggestion.drop(at)),
                Standing::Kept | Standing::Declined | Standing::Open(_) => 0,
            },
        }
    }

    /// Where each name stands on each target, as a word.
    fn standings(&self) -> Vec<&'static str> {
        let reading = self.read();
        (0..4)
            .flat_map(|n| NAMES.map(|name| (n, name)))
            .map(
                |(n, name)| match reading.standing(&target(n), &topic(name)) {
                    Standing::Kept => "kept",
                    Standing::Suggested(_) => "suggested",
                    Standing::Declined => "declined",
                    Standing::Open(_) => "open",
                },
            )
            .collect()
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
    /// The secretary's suggestion, where the topic is open.
    Suggest(usize, usize),
    /// The secretary's drop of its own suggestion.
    Drop(usize, usize),
}

impl Op {
    fn secretarys(&self) -> bool {
        matches!(self, Op::Suggest(..) | Op::Drop(..))
    }
}

/// The owner's ops and the secretary's: the box's door takes only the poster's own changes, so
/// the door's laws run over [`op`] alone.
fn any_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => op(),
        2 => (0usize..4, 0usize..4).prop_map(|(n, name)| Op::Suggest(n, name)),
        1 => (0usize..4, 0usize..4).prop_map(|(n, name)| Op::Drop(n, name)),
    ]
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

    /// Devices write apart and sync. No write of the secretary changes where a topic stands that
    /// the owner set or took off on that device, and the owner's add of a suggestion keeps it.
    #[test]
    fn the_secretary_never_undoes_the_owners_choice(script in proptest::collection::vec((0usize..3, any_op(), 0usize..3), 0..30)) {
        let mut all = devices(3);
        for (device, op, from) in &script {
            let before = all[*device].standings();
            let suggested = if let Op::Add(n, name) = op {
                matches!(all[*device].read().standing(&target(*n), &topic(NAMES[*name])), Standing::Suggested(_))
            } else {
                false
            };
            all[*device].apply(op);
            let after = all[*device].standings();
            if op.secretarys() {
                for (was, is) in before.iter().zip(&after) {
                    if matches!(*was, "kept" | "declined") {
                        prop_assert_eq!(was, is, "{:?}", op);
                    }
                }
            }
            if let (true, Op::Add(n, name)) = (suggested, op) {
                let reading = all[*device].read();
                prop_assert!(matches!(reading.standing(&target(*n), &topic(NAMES[*name])), Standing::Kept));
            }
            if from != device {
                all[*device].take(&all[*from]);
            }
        }
    }

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

/// The trust of a chain whose root trusts the cast's phone for every device's actor.
static TRUSTING: std::sync::LazyLock<suzatary_signatures::signatures::Trusting> =
    std::sync::LazyLock::new(|| {
        let actors: Vec<String> = (0..9).map(|n| format!("device{n}")).collect();
        let actors: Vec<&str> = actors.iter().map(String::as_str).collect();
        trusting_of(&[&suzatary_core::stores::phone_trusted(&actors)])
    });

fn trusting_of(
    chains: &[&dyn suzatary_store::store::Prodrome<suzatary_record::record::TodoEvent>],
) -> suzatary_signatures::signatures::Trusting {
    suzatary_signatures::signatures::Trusting::held(chains, &suzatary_core::cast::keys::root())
        .expect("reads")
}

/// The phone's signature of `object`, as the object a tab posts beside it.
fn signature<E: Schema>(object: &Envelope<E>) -> Envelope<E> {
    let key = suzatary_core::cast::keys::secret(suzatary_core::cast::keys::PHONE);
    Envelope::Signed(key.sign(&seal_hash(object)))
}

/// What the box takes when `device` posts every object it holds of its own actor's, in each
/// store, each signed by the device first: how many objects it took.
fn post(device: &Device, the_box: &Device) -> usize {
    posted(
        &device.memberships,
        &the_box.memberships,
        device.actor.actor(),
    ) + posted(&device.aliases, &the_box.aliases, device.actor.actor())
}

fn posted<E: Schema<Vocabulary: Default>>(
    from: &MemoryStore<E>,
    to: &MemoryStore<E>,
    actor: &Actor,
) -> usize {
    let own: Vec<Envelope<E>> = from
        .held()
        .expect("held")
        .dag
        .objects()
        .values()
        .filter(|object| object.event().is_some_and(|event| event.actor() == actor))
        .cloned()
        .collect();
    let signed: std::collections::BTreeMap<_, _> = own
        .iter()
        .map(signature)
        .map(|signature| {
            (
                seal_hash(&signature),
                canonical_envelope(&signature).into_bytes(),
            )
        })
        .collect();
    from.receive(signed.keys().cloned().collect(), &|name| {
        signed.get(name).cloned()
    })
    .expect("the device keeps its signatures");
    let mine: Vec<String> = own
        .iter()
        .map(canonical_envelope)
        .chain(
            signed
                .values()
                .map(|bytes| String::from_utf8(bytes.clone()).expect("a print")),
        )
        .collect();
    let held: BTreeSet<prodrome_core::event::Hash> = to
        .held()
        .expect("held")
        .dag
        .objects()
        .keys()
        .cloned()
        .collect();
    let received = suzatary_core::sync::receive_changes::<E>(&held, &mine, actor, &TRUSTING);
    let prints: std::collections::BTreeMap<_, _> = received
        .outcome
        .taken
        .iter()
        .map(|(name, print)| (name.clone(), print.clone().into_bytes()))
        .collect();
    to.receive(prints.keys().cloned().collect(), &|name| {
        prints.get(name).cloned()
    })
    .expect("the box takes what its door took");
    received.outcome.taken.len()
}

/// A topic store the box began, and what a tab of `device0` posts to it.
struct Door {
    held: BTreeSet<prodrome_core::event::Hash>,
    ada: Actor,
    /// `device0`'s topic, resting on the genesis.
    mine: Envelope<MembershipEvent>,
    /// `device0`'s change resting on `mine`.
    child: Envelope<MembershipEvent>,
    /// `device0`'s change resting on an object nobody holds.
    orphan: Envelope<MembershipEvent>,
    /// The same topic, written as `triage`.
    theirs: Envelope<MembershipEvent>,
    /// `device0`'s change resting on `theirs`.
    on_theirs: Envelope<MembershipEvent>,
}

impl Door {
    fn new() -> Self {
        use prodrome_core::change::mk_change;
        let genesis: Envelope<MembershipEvent> =
            Envelope::Genesis(mk_genesis("topics", &"0".repeat(32)).expect("a genesis"));
        let begun = seal_hash(&genesis);
        let ada = Actor::new("device0").expect("an actor");
        let change = |actor: &Actor, deps: Vec<prodrome_core::event::Hash>, add: bool| {
            let (target, topic, at) = (target(0), topic("travel"), at(1 + u32::from(!add)));
            let event = if add {
                prodrome_core::tags::membership::added::<Topics>(target, topic, at, actor.clone())
            } else {
                prodrome_core::tags::membership::removed::<Topics>(target, topic, at, actor.clone())
            };
            Envelope::Change(mk_change(begun.clone(), deps, event).expect("a change"))
        };
        let mine = change(&ada, Vec::new(), true);
        let unheld = prodrome_core::event::Hash::new("f".repeat(64)).expect("a name");
        let theirs = change(&Actor::new("triage").expect("an actor"), Vec::new(), true);
        Self {
            held: [begun.clone()].into(),
            child: change(&ada, vec![seal_hash(&mine)], false),
            orphan: change(&ada, vec![unheld], true),
            on_theirs: change(&ada, vec![seal_hash(&theirs)], false),
            theirs,
            mine,
            ada,
        }
    }

    /// What a post of `prints` would take, under `trusting`.
    fn post(
        &self,
        prints: &[String],
        trusting: &suzatary_signatures::signatures::Trusting,
    ) -> suzatary_core::sync::Received {
        suzatary_core::sync::receive_changes::<MembershipEvent>(
            &self.held, prints, &self.ada, trusting,
        )
    }

    /// Why a post of `prints` was refused whole.
    fn refused(&self, prints: &[String]) -> Vec<suzatary_core::sync::Reason> {
        let received = self.post(prints, &TRUSTING);
        assert!(received.outcome.taken.is_empty() && received.outcome.already.is_empty());
        received
            .outcome
            .refused
            .into_iter()
            .map(|r| r.reason)
            .collect()
    }
}

fn print(object: &Envelope<MembershipEvent>) -> String {
    canonical_envelope(object)
}

/// The box takes a tab's topic only as a change written as the poster, resting on what it
/// holds: a change naming another actor, a prodrome, a legacy object and bytes that are not one
/// canonical object are each refused, a change resting on an unheld object waits; a child posted
/// before its parent is taken with it, parents first; an object it holds is answered as held
/// whoever posts it.
#[test]
fn the_box_door_takes_only_the_posters_changes() {
    use suzatary_core::sync::Reason;
    let door = Door::new();
    assert!(matches!(
        door.refused(&[print(&door.theirs), print(&signature(&door.theirs))])
            .as_slice(),
        [
            Reason::ForeignActor { .. },
            Reason::RestsOn {
                kind: RefusalKind::Final,
                ..
            }
        ]
    ));
    let other: Envelope<MembershipEvent> =
        Envelope::Genesis(mk_genesis("topics", &"1".repeat(32)).expect("a genesis"));
    assert_eq!(door.refused(&[print(&other)]), vec![Reason::BoxOnly]);
    let legacy: Envelope<MembershipEvent> = prodrome_core::event::mk_sealed(
        None,
        prodrome_core::tags::membership::added::<Topics>(
            target(0),
            topic("travel"),
            at(1),
            door.ada.clone(),
        ),
    );
    assert_eq!(
        door.refused(&[print(&legacy)]),
        vec![Reason::NotAChange("Sealed")]
    );
    assert!(matches!(
        door.refused(&[format!("{} ", print(&door.mine))])
            .as_slice(),
        [Reason::NotAnObject(_)]
    ));
    assert!(matches!(
        door.refused(&[print(&door.orphan)]).as_slice(),
        [Reason::NotYetHeld(_)]
    ));
    let both = door.post(
        &[
            print(&signature(&door.child)),
            print(&door.child),
            print(&door.mine),
            print(&signature(&door.mine)),
        ],
        &TRUSTING,
    );
    let names: Vec<_> = both
        .outcome
        .taken
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    assert_eq!(
        names,
        vec![
            seal_hash(&door.mine),
            seal_hash(&signature(&door.mine)),
            seal_hash(&door.child),
            seal_hash(&signature(&door.child)),
        ]
    );
    let again = suzatary_core::sync::receive_changes::<MembershipEvent>(
        &door
            .held
            .iter()
            .cloned()
            .chain([seal_hash(&door.theirs)])
            .collect(),
        &[print(&door.theirs)],
        &door.ada,
        &TRUSTING,
    );
    assert_eq!(again.outcome.already, vec![seal_hash(&door.theirs)]);
}

/// A change is taken only beside a signature of it by a key every chain trusts for the poster:
/// an unsigned change and one signed by an untrusted key are refused until trusted, a forged
/// signature and a key revoked on any chain for good, and a signature only with its change. An
/// object resting on a refused one is refused as it is. Only a refusal for good of a change a
/// trusted key signed is recorded, once per post: never a revoked key's, never a waiting one.
#[test]
fn the_box_door_takes_a_change_only_with_its_writers_signature() {
    use suzatary_core::sync::Reason;
    use suzatary_signatures::signatures::Unproven;
    let door = Door::new();
    let mine = &door.mine;
    assert_eq!(
        door.refused(&[print(mine)]),
        vec![Reason::Unproven(Unproven::Unsigned)]
    );
    let stranger = prodrome_core::sign::Secret::new(&"7".repeat(64)).expect("a seed");
    let untrusted: Envelope<MembershipEvent> = Envelope::Signed(stranger.sign(&seal_hash(mine)));
    assert!(matches!(
        door.refused(&[print(mine), print(&untrusted)]).as_slice(),
        [
            Reason::Unproven(Unproven::Untrusted { .. }),
            Reason::RestsOn {
                kind: RefusalKind::Trust,
                ..
            }
        ]
    ));
    let Envelope::Signed(mut forged) = signature(mine) else {
        unreachable!("a signature")
    };
    forged.signature = stranger.sign(&seal_hash(mine)).signature;
    assert!(matches!(
        door.refused(&[print(mine), print(&Envelope::Signed(forged))])
            .as_slice(),
        [
            Reason::Unproven(Unproven::Forged),
            Reason::RestsOn {
                kind: RefusalKind::Final,
                ..
            }
        ]
    ));
    assert_eq!(
        door.refused(&[print(&signature(mine))]),
        vec![Reason::NotYetHeld(seal_hash(mine))]
    );
    let lone = suzatary_core::sync::receive_changes::<MembershipEvent>(
        &door.held.iter().cloned().chain([seal_hash(mine)]).collect(),
        &[print(&signature(mine))],
        &door.ada,
        &TRUSTING,
    );
    let reasons: Vec<_> = lone.outcome.refused.into_iter().map(|r| r.reason).collect();
    assert_eq!(reasons, vec![Reason::Unaccompanied]);

    let revoking = suzatary_core::stores::phone_trusted(&["device0"]);
    suzatary_signatures::signatures::revoke(
        &revoking,
        &suzatary_core::cast::keys::secret(suzatary_core::cast::keys::ROOT),
        &door.ada,
        &suzatary_core::cast::keys::secret(suzatary_core::cast::keys::PHONE).public(),
    )
    .expect("revokes");
    let fine = suzatary_core::stores::phone_trusted(&["device0"]);
    let revoked = door.post(
        &[print(mine), print(&signature(mine))],
        &trusting_of(&[&fine, &revoking]),
    );
    assert!(matches!(
        revoked.outcome.refused[0].reason,
        Reason::Unproven(Unproven::Revoked { .. })
    ));
    assert!(revoked.recorded.is_empty(), "a revoked key records nothing");

    let on_theirs = &door.on_theirs;
    let rests = door.post(
        &[
            print(&door.theirs),
            print(on_theirs),
            print(on_theirs),
            print(on_theirs),
            print(&signature(on_theirs)),
        ],
        &TRUSTING,
    );
    assert!(
        matches!(
            rests.recorded.as_slice(),
            [(name, Reason::RestsOn { kind: RefusalKind::Final, .. })] if *name == seal_hash(on_theirs)
        ),
        "recorded once, however often the post names it"
    );
    let orphan = &door.orphan;
    let waits = door.post(&[print(orphan), print(&signature(orphan))], &TRUSTING);
    assert!(matches!(
        waits.outcome.refused[0].reason,
        Reason::NotYetHeld(_)
    ));
    assert!(
        waits.recorded.is_empty(),
        "what may still be taken is recorded nowhere"
    );
}

/// THE FIRST DEVICE'S WRITES ARE TAKEN ONCE IT IS TRUSTED. A chain of two changes, each beside
/// its signature, posted by a device no chain trusts yet, is refused until trusted, all four
/// objects; the tab keeps all four to post again and has nothing to sign again; posted again
/// once the device is trusted, all four are taken.
#[test]
fn law_writes_made_before_trust_are_taken_after_it() {
    use suzatary_store::replica::objects::{Answer, Object, Objects, Refusal};
    let door = Door::new();
    let schema = MembershipVocabulary::default();
    let mut tab = Objects::default();
    tab.write(
        [
            door.mine.clone(),
            signature(&door.mine),
            door.child.clone(),
            signature(&door.child),
        ]
        .iter()
        .map(|o| Object::read::<MembershipEvent>(&schema, print(o)))
        .collect(),
    );
    let posted = |tab: &Objects| -> Vec<String> {
        tab.unsent_pieces::<MembershipEvent>(&schema, 512)
            .concat()
            .iter()
            .map(|o| o.literal().to_owned())
            .collect()
    };
    let untrusted = trusting_of(&[&suzatary_core::stores::phone_trusted(&[])]);
    let first = door.post(&posted(&tab), &untrusted);
    assert!(first.outcome.taken.is_empty());
    assert_eq!(first.outcome.refused.len(), 4);
    assert!(
        first
            .outcome
            .refused
            .iter()
            .all(|r| r.reason.kind() == RefusalKind::Trust),
        "{:?}",
        first.outcome.refused
    );
    assert!(first.recorded.is_empty());
    tab.answered(first.outcome.refused.iter().filter_map(|r| {
        Some((
            r.name.clone()?,
            Answer::Refused {
                kind: Refusal::Trust,
                reason: r.reason.said(),
            },
        ))
    }));
    assert!(tab.unsigned::<MembershipEvent>(&schema).is_empty());
    assert_eq!(posted(&tab).len(), 4);
    let second = door.post(&posted(&tab), &TRUSTING);
    assert_eq!(
        second.outcome.taken.len(),
        4,
        "{:?}",
        second.outcome.refused
    );
}
