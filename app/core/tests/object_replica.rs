//! A store of objects as a device holds it (`replica::objects`): every move is a join, so the
//! frames of any history, taken in any order and any number of times, rebuild the replica that
//! made them, which is the law two tabs converge by; its snapshot and frames read back to what
//! they printed; and what the replica wrote stays readable and unsent until the box answers,
//! whatever the rows do, a refusal for good holding it back with everything resting on it.

use std::collections::BTreeSet;

use prodrome_core::event::{Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_core::genesis::mk_genesis;
use prodrome_core::literal::Datetime;
use prodrome_core::store::{MemoryStore, Replica as _};
use proptest::prelude::*;

use suzatary_core::conversation_id::ConversationId;
use suzatary_core::replica::frontier::Frontier;
use suzatary_core::replica::held::{Node, Said};
use suzatary_core::replica::objects::{Answer, Move, Object, Objects, Refusal, frame, unframe};
use suzatary_core::secretary::topics::reading::{
    AliasEvent, MembershipEvent, MembershipVocabulary,
};
use suzatary_core::secretary::topics::topic::Topic;
use suzatary_core::snapshot::{from_snapshot, snapshot};
use suzatary_core::topic_target::Target;
use suzatary_core::wire::store::SinceRead;

/// The box's store: one genesis, then `n` memberships, each written over the one before.
fn universe(n: u32) -> Vec<Object> {
    let memberships: MemoryStore<MembershipEvent> = MemoryStore::default();
    let aliases: MemoryStore<AliasEvent> = MemoryStore::default();
    begin(&memberships);
    begin(&aliases);
    for i in 0..n {
        let reading = read(&memberships, &aliases);
        let at = Datetime::new(2026, 11, 3, 9, i % 60, 0, 0).expect("an instant");
        let target = Target::Round {
            conversation: ConversationId::new("default").expect("a slug"),
            turn: usize::try_from(i).expect("small"),
        };
        let topic = Topic::new("travel").expect("a topic");
        if let Some(event) = reading.suggestion(&target, &topic, at) {
            memberships.append(event).expect("appended");
        }
    }
    let held = memberships.held().expect("held");
    held.dag
        .nodes()
        .expect("nodes")
        .iter()
        .map(|node| {
            let print = canonical_envelope(held.dag.get(node.name()).expect("held"));
            Object::read::<MembershipEvent>(&MembershipVocabulary::default(), print)
        })
        .collect()
}

fn begin<E: prodrome_core::schema::Schema>(store: &MemoryStore<E>) {
    let genesis: Envelope<E> =
        Envelope::Genesis(mk_genesis("topics", &"0".repeat(32)).expect("a genesis"));
    let print = canonical_envelope(&genesis).into_bytes();
    let name = seal_hash(&genesis);
    store
        .receive([name].into(), &|_| Some(print.clone()))
        .expect("begins");
}

fn read(
    memberships: &MemoryStore<MembershipEvent>,
    aliases: &MemoryStore<AliasEvent>,
) -> suzatary_core::secretary::topics::reading::Reading {
    let m = memberships
        .held()
        .expect("held")
        .dag
        .nodes()
        .expect("nodes");
    let a = aliases.held().expect("held").dag.nodes().expect("nodes");
    suzatary_core::secretary::topics::reading::of(&m, &a).expect("one genesis")
}

/// The box's answer holding the first `held` objects of `all`.
fn said(all: &[Object], held: usize, epoch: u64, digest: u8) -> Said<Object> {
    let nodes: Vec<Object> = all[..held].to_vec();
    let named: BTreeSet<&Hash> = nodes.iter().flat_map(Node::rests_on).collect();
    let heads = nodes
        .iter()
        .map(|n| n.name().clone())
        .filter(|n| !named.contains(n))
        .collect();
    let geneses = all[..1].iter().map(|n| n.name().clone()).collect();
    Said {
        heads,
        geneses,
        epoch,
        nodes,
        since: SinceRead::Known,
        digest: Hash::new(hex::encode([digest; 32])).expect("a name"),
        count: u64::try_from(held).expect("small"),
    }
}

/// One step of a replica's history.
#[derive(Debug, Clone)]
enum Op {
    Piece { held: usize, epoch: u64, digest: u8 },
    Land,
    StartOver,
    Write(usize),
    Answer(usize, u8),
}

fn op(objects: usize) -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (1..=objects, 1u64..3, any::<u8>()).prop_map(|(held, epoch, digest)| Op::Piece { held, epoch, digest }),
        3 => Just(Op::Land),
        1 => Just(Op::StartOver),
        2 => (0..objects).prop_map(Op::Write),
        2 => (0..objects, 0u8..3).prop_map(|(i, a)| Op::Answer(i, a)),
    ]
}

fn answer(n: u8) -> Answer {
    match n {
        0 => Answer::Taken,
        1 => Answer::Refused {
            kind: Refusal::Trust,
            reason: "untrusted".to_owned(),
        },
        _ => Answer::Refused {
            kind: Refusal::Final,
            reason: "no".to_owned(),
        },
    }
}

fn step(replica: &mut Objects, all: &[Object], op: &Op) -> Move {
    match op {
        Op::Piece {
            held,
            epoch,
            digest,
        } => replica.piece(said(all, *held, *epoch, *digest), 7),
        Op::Land => replica.land(),
        Op::StartOver => replica.start_over(),
        Op::Write(i) => replica.write(vec![all[*i].clone()]),
        Op::Answer(i, a) => replica.answered([(all[*i].name().clone(), answer(*a))]),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The frames of a history, shuffled and each taken twice, rebuild the replica that made
    /// them, and its snapshot and frames read back to what they printed.
    #[test]
    fn law_the_moves_of_a_store_of_objects_are_joins(
        ops in prop::collection::vec(op(6), 0..24),
        order in any::<u64>(),
    ) {
        let all = universe(5);
        let mut replica = Objects::default();
        let moves: Vec<Move> = ops.iter().map(|o| step(&mut replica, &all, o)).collect();

        let frames: Vec<Vec<u8>> = moves.iter().map(|m| frame(m).expect("prints")).collect();
        for (m, f) in moves.iter().zip(&frames) {
            prop_assert_eq!(&unframe(f).expect("reads").expect("this form"), m);
        }
        let mut shuffled: Vec<&Vec<u8>> = frames.iter().chain(&frames).collect();
        let mut seed = order;
        for i in (1..shuffled.len()).rev() {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            shuffled.swap(i, usize::try_from(seed % (i as u64 + 1)).expect("small"));
        }
        let mut other = Objects::default();
        for f in shuffled {
            other.apply(&unframe(f).expect("reads").expect("this form"));
        }
        prop_assert_eq!(&other, &replica);

        let back: Objects = from_snapshot(&snapshot(&replica).expect("prints")).expect("reads");
        prop_assert_eq!(&back, &replica);
        prop_assert_eq!(back.standing(), replica.standing());
    }
}

/// What a replica wrote is read and posted until the box takes it, through a reset of the rows;
/// a refusal for good holds it back from both, with what rests on it; one until trusted is
/// posted again; an answer only climbs.
#[test]
fn law_what_a_replica_wrote_stands_until_the_box_answers() {
    let all = universe(1);
    let (genesis, added) = (all[0].clone(), all[1].clone());
    assert_eq!(added.rests_on(), [genesis.name().clone()]);
    let count = |objects: Vec<&Object>| objects.len();

    let mut replica = Objects::default();
    replica.write(vec![genesis.clone(), added.clone()]);
    assert_eq!(count(replica.unsent()), 2);
    assert_eq!(count(replica.standing()), 2);

    replica.piece(said(&all, 1, 1, 1), 1);
    replica.land();
    assert_eq!(count(replica.unsent()), 1, "the box holds the genesis");
    assert_eq!(count(replica.standing()), 2);
    replica.start_over();
    assert_eq!(
        count(replica.unsent()),
        2,
        "the rows reset, the writes stay"
    );
    assert_eq!(count(replica.standing()), 2);

    replica.answered([(genesis.name().clone(), answer(1))]);
    assert_eq!(
        count(replica.unsent()),
        2,
        "a refusal until trusted is posted again"
    );
    replica.answered([(genesis.name().clone(), answer(2))]);
    assert!(replica.unsent().is_empty(), "the second rests on the first");
    assert!(replica.standing().is_empty());
    replica.answered([(genesis.name().clone(), answer(1))]);
    assert!(replica.standing().is_empty(), "an answer only climbs");

    let mut taken = Objects::default();
    taken.write(vec![genesis.clone()]);
    taken.answered([(genesis.name().clone(), Answer::Taken)]);
    assert!(taken.unsent().is_empty());
    assert_eq!(count(taken.standing()), 1);
    assert!(matches!(
        taken.rows().holding().replica().frontier(),
        Frontier::Nothing
    ));
}

/// A frame of the lines' form is passed over, never misread as a move of objects.
#[test]
fn a_frame_of_another_form_is_passed_over() {
    let lines = suzatary_core::secretary::line_form::frame(
        &suzatary_core::secretary::line_form::LineFrame {
            delta: suzatary_core::replica::pulling::Delta::StartedOver { generation: 1 },
            husked: Vec::new(),
        },
    )
    .expect("prints");
    assert!(unframe(&lines).expect("a form").is_none());
}

/// The box's store in each epoch: before a migration, `before`; after it, the last membership
/// gone and another in its place, so the two share the genesis and the rest.
fn epochs() -> (Vec<Object>, Vec<Object>) {
    let before = universe(4);
    let later = universe(5);
    let mut after: Vec<Object> = before[..before.len() - 1].to_vec();
    after.push(later[later.len() - 1].clone());
    (before, after)
}

const OLD: u64 = 1;
const LATER: u64 = 2;

/// The box's answer at `epoch` sending `sent`, naming the heads of `store`, all it held then.
fn served(store: &[Object], sent: Vec<Object>, epoch: u64) -> Said<Object> {
    let named: BTreeSet<&Hash> = store.iter().flat_map(Node::rests_on).collect();
    Said {
        heads: store
            .iter()
            .map(|n| n.name().clone())
            .filter(|n| !named.contains(n))
            .collect(),
        geneses: store[..1].iter().map(|n| n.name().clone()).collect(),
        epoch,
        nodes: sent,
        since: SinceRead::Known,
        digest: Hash::new(hex::encode([u8::try_from(epoch).expect("small"); 32])).expect("a name"),
        count: 0,
    }
}

/// What the box answers a replica holding `held`: the objects of `store` not under its heads.
fn lacked(store: &[Object], held: &Objects) -> Vec<Object> {
    let holding = held.rows().holding();
    let reading = holding.replica().read();
    let mut under: BTreeSet<Hash> = BTreeSet::new();
    let mut left: Vec<Hash> = reading.heads().into_iter().collect();
    while let Some(name) = left.pop() {
        if let Some(o) = store.iter().find(|o| o.name() == &name)
            && under.insert(name)
        {
            left.extend(o.rests_on().iter().cloned());
        }
    }
    store
        .iter()
        .filter(|o| !under.contains(o.name()))
        .cloned()
        .collect()
}

/// A down-set of `store`, parents first: its first `n` objects, or all of them.
fn down(store: &[Object], n: usize) -> Vec<Object> {
    store[..n.min(store.len())].to_vec()
}

/// What another tab's moves bring: each taken into `device`.
fn took(device: &mut Objects, moves: impl IntoIterator<Item = Move>) {
    for m in moves {
        device.apply(&m);
    }
}

/// Whether the landed row holds only what the box could have sent in its word's epoch.
fn could_have_sent(device: &Objects, before: &[Object], after: &[Object]) -> Result<(), String> {
    let landed = device.rows().landed().replica();
    let names: BTreeSet<&Hash> = landed.nodes().map(Node::name).collect();
    let Some(word) = landed.spoken() else {
        return if names.is_empty() {
            Ok(())
        } else {
            Err("objects held beside no word of the box".to_owned())
        };
    };
    let store = if word.epoch == LATER { after } else { before };
    let served: BTreeSet<&Hash> = store.iter().map(Node::name).collect();
    if let Some(foreign) = names.iter().find(|n| !served.contains(*n)) {
        return Err(format!(
            "a word of epoch {} beside {foreign}, which the box did not hold then",
            word.epoch
        ));
    }
    if landed
        .nodes()
        .any(|o| o.rests_on().iter().any(|p| !names.contains(p)))
    {
        return Err("an object without its parents".to_owned());
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// In whatever order an answer of the old epoch, pieces kept from before and from another
    /// tab, another tab's pull and reset and this tab's pull land, the landed row never holds
    /// the box's later word beside an object the box no longer holds, and a pull ends holding the
    /// box's store whole.
    #[test]
    fn law_the_landed_row_never_holds_a_later_word_beside_an_object_the_box_dropped(
        order in prop::collection::vec((0usize..8, 1usize..6), 1..16),
    ) {
        let (before, after) = epochs();
        let mut device = Objects::default();
        let mut fresh = Objects::default();
        took(&mut device, [fresh.piece(served(&before, before.clone(), OLD), 1), fresh.land()]);
        let pull = |device: &mut Objects| {
            if device.rows().overtaken(LATER) {
                device.start_over();
            }
            let sent = lacked(&after, device);
            device.piece(served(&after, sent, LATER), 9);
            device.land();
        };
        let mut steps: Vec<usize> = Vec::new();
        for (step, n) in order.iter().copied().chain([(7, 1)]) {
            steps.push(step);
            let mut fresh = Objects::default();
            match step {
                0 => took(&mut device, [fresh.piece(served(&before, down(&before, n), OLD), 2), fresh.land()]),
                1 => took(&mut device, [fresh.piece(served(&before, vec![before[before.len() - 1].clone()], OLD), 3)]),
                2 => {
                    let mut other = device.clone();
                    if other.rows().overtaken(LATER) {
                        device.apply(&other.start_over());
                    }
                    let sent = lacked(&after, &other);
                    device.apply(&other.piece(served(&after, down(&sent, n), LATER), 4));
                }
                3 => {
                    if device.rows().landed().replica().spoken().is_some_and(|w| w.epoch == OLD) {
                        took(&mut device, [fresh.piece(served(&after, vec![after[after.len() - 1].clone()], LATER), 4)]);
                    }
                }
                4 => took(&mut device, [fresh.piece(served(&after, after.clone(), LATER), 5), fresh.land()]),
                5 => took(&mut device, [fresh.piece(served(&before, before.clone(), OLD), 6), fresh.land()]),
                6 => {
                    let mut other = device.clone();
                    device.apply(&other.start_over());
                }
                _ => pull(&mut device),
            }
            prop_assert_eq!(could_have_sent(&device, &before, &after), Ok(()), "after {:?}", steps);
        }
        let landed: BTreeSet<Hash> = device.rows().landed().replica().nodes().map(|o| o.name().clone()).collect();
        let whole: BTreeSet<Hash> = after.iter().map(|o| o.name().clone()).collect();
        prop_assert_eq!(landed, whole, "after {:?}", steps);
    }
}
