//! The lines' rows are a replica of the box's skeleton, joined as a semilattice and moved only
//! by deltas: a row's join is commutative, associative and idempotent, with the empty row of
//! generation 0 its unit; the deltas of any history, the joins among them taken in any order
//! and any number of times, rebuild the rows it left; and the readings are told the landed row
//! after every move, whatever moved it, so they hold its readable nodes and its keys alone.

use std::collections::BTreeSet;

use proptest::prelude::*;
use sha2::{Digest, Sha256};

use prodrome_core::event::Hash;
use suzatary_core::replica::frontier::Frontier;
use suzatary_core::replica::held::{Held, Node, Said};
use suzatary_core::replica::pulling::Delta;
use suzatary_core::replica::row::Row;
use suzatary_core::secretary::held_replica::{HeldReplica, Moved};
use suzatary_core::secretary::key_ring::KeyRing;
use suzatary_core::secretary::line_form::{LineDelta, LinePulling, frame, unframe};
use suzatary_core::secretary::locked_text::KeyName;
use suzatary_core::secretary::outlines::Parsed;
use suzatary_core::secretary::step::Step;
use suzatary_core::snapshot::{from_snapshot, snapshot};
use suzatary_core::wire::{self, Heads, NodeHeader, SinceRead, SkeletonNode, SkeletonPayload};

type LineRow = Row<Parsed, KeyRing>;

const LINES: usize = 10;
const KEYS: u8 = 4;

fn name(seed: u8) -> Hash {
    Hash::new(hex::encode(Sha256::digest([seed]))).expect("a name")
}

fn key(seed: u8) -> (KeyName, String) {
    let bytes = [seed + 1; 32];
    let named = KeyName::new(hex::encode(Sha256::digest(bytes))).expect("a name");
    (named, hex::encode(bytes))
}

fn ring(seeds: &[u8]) -> KeyRing {
    KeyRing::of(&seeds.iter().map(|s| key(*s)).collect())
}

/// Which of the two prodromes line `i` is in.
fn genesis_of(i: usize) -> u8 {
    u8::from(i >= 7)
}

/// The box's store: two geneses, then ten lines, each resting on its genesis and on the line
/// before it in the same prodrome, parents first.
fn universe() -> Vec<Parsed> {
    let genesis = |seed: u8| SkeletonNode {
        hash: name(seed).as_str().to_owned(),
        parents: Vec::new(),
        size: 1,
        header: NodeHeader::Structure {
            kind: "Genesis".to_owned(),
        },
    };
    let mut nodes = vec![genesis(0), genesis(1)];
    for i in 0..LINES {
        let mut parents = vec![name(genesis_of(i)).as_str().to_owned()];
        if i > 0 && genesis_of(i - 1) == genesis_of(i) {
            parents.push(name(line_seed(i - 1)).as_str().to_owned());
        }
        parents.sort();
        let seed = line_seed(i);
        nodes.push(SkeletonNode {
            hash: name(seed).as_str().to_owned(),
            parents,
            size: 10,
            header: NodeHeader::Line {
                conversation: ["first", "second", "third"][i % 3].to_owned(),
                at: format!("2026-10-31T10:00:{i:02}"),
                kind: "Said".to_owned(),
                keys: vec![
                    key(u8::try_from(i).expect("small") % KEYS)
                        .0
                        .as_str()
                        .to_owned(),
                ],
                step: Step::Asks,
            },
        });
    }
    nodes
        .into_iter()
        .map(|n| Parsed::of(n).expect("a node"))
        .collect()
}

fn line_seed(i: usize) -> u8 {
    10 + u8::try_from(i).expect("small")
}

/// The heads of a store holding `nodes`.
fn heads(nodes: &[Parsed]) -> BTreeSet<Hash> {
    let named: BTreeSet<&Hash> = nodes.iter().flat_map(Node::rests_on).collect();
    nodes
        .iter()
        .map(|n| n.name().clone())
        .filter(|n| !named.contains(n))
        .collect()
}

/// One answer of the box: the first `held` nodes of its store, with its true heads.
#[derive(Debug, Clone)]
struct Answer {
    held: usize,
    geneses: Vec<u8>,
    epoch: u64,
    unknown: bool,
    digest: u8,
    fetched_at: u64,
    keys: Vec<u8>,
}

impl Answer {
    fn said(&self) -> Said<Parsed> {
        let nodes: Vec<Parsed> = universe().into_iter().take(self.held).collect();
        Said {
            heads: heads(&nodes),
            geneses: self.geneses.iter().map(|s| name(*s)).collect(),
            epoch: self.epoch,
            nodes,
            since: if self.unknown {
                SinceRead::Unknown
            } else {
                SinceRead::Known
            },
            digest: name(100 + self.digest),
            count: u64::try_from(self.held).expect("small"),
        }
    }
}

impl Answer {
    fn payload(&self) -> SkeletonPayload {
        let said = self.said();
        SkeletonPayload {
            heads: Heads::These {
                heads: said.heads.iter().map(|h| h.as_str().to_owned()).collect(),
            },
            geneses: said.geneses.iter().map(|h| h.as_str().to_owned()).collect(),
            epoch: said.epoch,
            nodes: said.nodes.iter().map(|n| n.given().clone()).collect(),
            since: said.since,
            frontier: wire::Frontier {
                digest: said.digest.as_str().to_owned(),
                count: self.held,
            },
            keys: std::collections::BTreeMap::new(),
            rest: 0,
            size: 0,
            weight: 0,
        }
    }
}

fn answer() -> impl Strategy<Value = Answer> {
    (
        0..=LINES + 2,
        prop::sample::select(vec![vec![], vec![0], vec![0, 1], vec![1]]),
        1u64..=3,
        prop::bool::weighted(0.2),
        0u8..3,
        0u64..4,
        prop::collection::vec(0..KEYS, 0..3),
    )
        .prop_map(
            |(held, geneses, epoch, unknown, digest, fetched_at, keys)| Answer {
                held,
                geneses,
                epoch,
                unknown,
                digest,
                fetched_at,
                keys,
            },
        )
}

/// A row: a generation and the answers it heard, from nothing.
fn row() -> impl Strategy<Value = LineRow> {
    (0u64..3, prop::collection::vec(answer(), 0..4)).prop_map(|(generation, answers)| {
        let mut keys = KeyRing::default();
        let mut replica = Held::empty(generation);
        for a in &answers {
            replica = replica.pulled(a.said(), a.fetched_at);
            keys = suzatary_core::replica::row::Keys::union(keys, &ring(&a.keys));
        }
        Row::of(replica, keys)
    })
}

#[derive(Debug, Clone)]
enum Op {
    Piece(Answer),
    Land,
    StartOver,
    Forget(Vec<u8>),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        5 => answer().prop_map(Op::Piece),
        3 => Just(Op::Land),
        1 => Just(Op::StartOver),
        1 => prop::collection::vec(0..KEYS, 1..3).prop_map(Op::Forget),
    ]
}

fn delta(rows: &mut LinePulling, op: &Op) -> LineDelta {
    match op {
        Op::Piece(a) => rows.piece(a.said(), ring(&a.keys), a.fetched_at),
        Op::Land => rows.land(),
        Op::StartOver => rows.start_over(),
        Op::Forget(seeds) => rows.forget(seeds.iter().map(|s| key(*s).0).collect()),
    }
}

fn printed(rows: &LinePulling) -> Vec<u8> {
    snapshot(rows).expect("prints")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn the_row_join_is_a_semilattice(a in row(), b in row(), c in row()) {
        let join = |x: &LineRow, y: &LineRow| x.clone().join(y);
        prop_assert_eq!(join(&a, &a), a.clone(), "idempotent");
        prop_assert_eq!(join(&a, &LineRow::empty(0)), a.clone(), "the empty row of generation 0 is its unit");
        prop_assert_eq!(join(&a, &b), join(&b, &a), "commutative");
        prop_assert_eq!(join(&join(&a, &b), &c), join(&a, &join(&b, &c)), "associative");
    }

    #[test]
    fn a_replicas_deltas_rebuild_it_the_joins_in_any_order_and_any_number_of_times(
        ops in prop::collection::vec(op(), 0..14),
        order in prop::collection::vec(any::<u64>(), 14),
    ) {
        let mut live = LinePulling::default();
        let deltas: Vec<LineDelta> = ops.iter().map(|op| delta(&mut live, op)).collect();
        // The joins between two purges, in another order, each taken twice.
        let mut segments: Vec<Vec<(u64, &LineDelta)>> = vec![Vec::new()];
        for (at, d) in deltas.iter().enumerate() {
            match d {
                Delta::Forgot(_) => segments.extend([vec![(0, d)], Vec::new()]),
                Delta::Piece { .. } | Delta::Landed { .. } | Delta::StartedOver { .. } => {
                    if let Some(last) = segments.last_mut() {
                        last.push((order[at], d));
                    }
                }
            }
        }
        let mut rebuilt = LinePulling::default();
        for mut segment in segments {
            segment.sort_by_key(|(k, _)| *k);
            for (_, d) in &segment {
                rebuilt.apply(d);
                if !matches!(d, Delta::Forgot(_)) {
                    rebuilt.apply(d);
                }
            }
        }
        prop_assert_eq!(printed(&rebuilt), printed(&live));
        // And through the device's form: each delta framed and read back.
        let mut read = LinePulling::default();
        for d in &deltas {
            read.apply(&unframe(&frame(d).expect("frames")).expect("reads").expect("of this form"));
        }
        prop_assert_eq!(printed(&read), printed(&live));
        let back: LinePulling = from_snapshot(&printed(&live)).expect("reads back");
        prop_assert_eq!(&back, &live, "a snapshot reads back as the rows it printed");
    }

    #[test]
    fn the_readings_are_told_the_landed_row_after_every_move(
        ops in prop::collection::vec(op(), 0..14),
        others in prop::collection::vec(op(), 0..6),
    ) {
        let mut held = HeldReplica::default();
        let mut other = LinePulling::default();
        let theirs: Vec<LineDelta> = others.iter().map(|op| delta(&mut other, op)).collect();
        let mut theirs = theirs.iter();
        for op in &ops {
            let was = held.rows().landed().clone();
            let moved = match op {
                Op::Piece(a) => {
                    held.piece(a.payload(), ring(&a.keys), a.fetched_at).expect("reads");
                    None
                }
                Op::Land => Some(held.land().moved),
                Op::StartOver => Some(held.start_over().moved),
                Op::Forget(seeds) => Some(held.forget(seeds.iter().map(|s| key(*s).0).collect()).moved),
            };
            let landed = held.rows().landed();
            let reading = landed.replica().read();
            let names: BTreeSet<&Hash> = reading.nodes().map(Node::name).collect();
            prop_assert_eq!(held.outlines().names().collect::<BTreeSet<_>>(), names, "the nodes read, and no other");
            prop_assert_eq!(held.lines.key_ring(), landed.keys(), "the row's keys");
            if let Some(moved) = moved {
                let anew = was.generation() != landed.generation() || was.replica().epoch() != landed.replica().epoch();
                prop_assert_eq!(anew, moved == Moved::All, "every conversation, exactly where the row was reset or heard a later epoch");
            }
            if let Some(d) = theirs.next() {
                held.apply(d);
            }
        }
    }
}

#[test]
fn a_frame_of_another_form_is_passed_over() {
    let mut rows = LinePulling::default();
    let mut bytes = frame(&rows.start_over()).expect("frames");
    bytes[0] ^= 1;
    assert!(matches!(unframe(&bytes), Ok(None)));
    assert_eq!(rows.landed().replica().frontier(), &Frontier::Nothing);
}
