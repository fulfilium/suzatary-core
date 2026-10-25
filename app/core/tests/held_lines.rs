//! The memoised reading of the lines is the plain one, whatever order the prints and their keys
//! arrive in and whatever is let go, and a new line costs its own leaf, its conversation and the
//! store, every other node being one hit.

use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;

use prodrome_core::dag::{Dag, Excluded};
use prodrome_core::event::{Actor, Hash, canonical_envelope};
use prodrome_core::literal::Datetime;
use prodrome_core::memo::Counts;

use std::sync::atomic::{AtomicU64, Ordering};

use suzatary_core::capability::{Entropy, Turn};
use suzatary_core::secretary::conversations::{Conversations, InMemory};
use suzatary_core::secretary::erasure::Erasure;
use suzatary_core::secretary::held_lines::HeldLines;
use suzatary_core::secretary::key_ring::{KeyRing, spelled};
use suzatary_core::secretary::line_event::{LineEvent, LineVocabulary};
use suzatary_core::secretary::locked_text::KeyName;
use suzatary_core::secretary::store::{ConversationId, Entry, Kind, NotAConversation, Stored};
use suzatary_core::secretary::transcript::{self, Transcript};
use suzatary_core::secretary::words::Words;
use suzatary_core::store::Doc as _;
use suzatary_core::store::Reads as _;

/// One step of a script: a turn said in a conversation, or an erasure of some of its lines. A turn
/// that `echoes` draws its keys as the last conversation opened drew its first, so one key names
/// slots of two conversations.
#[derive(Debug, Clone)]
enum Step {
    Said {
        conversation: usize,
        model: bool,
        echoes: bool,
    },
    Erase {
        conversation: usize,
        lines: Vec<usize>,
    },
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => (0usize..3, any::<bool>(), proptest::bool::weighted(0.2))
            .prop_map(|(conversation, model, echoes)| Step::Said { conversation, model, echoes }),
        1 => (0usize..3, proptest::collection::vec(0usize..8, 1..3))
            .prop_map(|(conversation, lines)| Step::Erase { conversation, lines }),
    ]
}

fn at(n: u32) -> Datetime {
    Datetime::new(2026, 10, 1, 9, n / 60 % 60, n % 60, 0).expect("an instant")
}

fn id(n: usize) -> ConversationId {
    ConversationId::new(format!("2026-10-01-{}", n + 1)).expect("a slug")
}

/// Keys drawn one count past the last, from a count a script may set back.
#[derive(Default)]
struct Rewound(AtomicU64);

impl Entropy for Rewound {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        let count = self.0.fetch_add(1, Ordering::Relaxed);
        for (at, byte) in bytes.iter_mut().enumerate() {
            *byte = count.to_be_bytes()[at % 8] ^ u8::try_from(at % 251).unwrap_or(0);
        }
        Ok(())
    }
}

/// The store a script writes, through the box's own writes, with the keys a sweep left.
fn written(script: &[Step]) -> InMemory {
    let held = InMemory::default();
    let entropy = Rewound::default();
    let store = Conversations {
        entropy: &entropy,
        ..held.conversations()
    };
    let mut started = BTreeSet::new();
    let mut opening = None;
    for (n, step) in (1u32..).zip(script) {
        let (Step::Said { conversation, .. } | Step::Erase { conversation, .. }) = step;
        if let (Step::Said { echoes: true, .. }, Some(count)) = (step, opening) {
            entropy.0.store(count, Ordering::Relaxed);
        }
        if started.insert(*conversation) {
            opening = Some(entropy.0.load(Ordering::Relaxed));
            transcript::start(
                store,
                &id(*conversation),
                &Entry::Opened {
                    at: at(n),
                    title: Words::kept(format!("Topic {conversation}")),
                    model: "a-cli/small".to_owned(),
                    kind: Kind::Topic,
                },
            )
            .expect("started");
        }
        match step {
            Step::Said {
                conversation,
                model,
                ..
            } => {
                let turn = if *model {
                    Turn::Model {
                        text: format!("Answer {n}.\n\nMore of answer {n}."),
                        calls: Vec::new(),
                    }
                } else {
                    Turn::Person {
                        text: format!("question {n}"),
                        references: Vec::new(),
                        attachments: Vec::new(),
                    }
                };
                let by = (!model).then(|| "ada".into());
                transcript::push(
                    store,
                    &id(*conversation),
                    &Entry::Said {
                        at: at(n),
                        turn,
                        by,
                    },
                )
                .expect("said");
            }
            Step::Erase {
                conversation,
                lines,
            } => {
                let read = transcript::read(store, &id(*conversation)).expect("read");
                let keys: BTreeSet<KeyName> = lines
                    .iter()
                    .filter_map(|&line| read.lines().get(line))
                    .flat_map(|written| written.line.keys().cloned())
                    .collect();
                if !keys.is_empty() {
                    let by = Erasure::Asked {
                        by: Actor::new("ada").expect("a name"),
                    };
                    transcript::erase(store, &id(*conversation), keys, by, at(n)).expect("erased");
                }
            }
        }
    }
    held
}

fn prints(held: &InMemory) -> Vec<(Hash, Vec<u8>)> {
    held.lines
        .objects()
        .expect("objects")
        .into_iter()
        .map(|(name, object)| (name, canonical_envelope(&object).into_bytes()))
        .collect()
}

fn keys(held: &InMemory) -> Vec<(KeyName, String)> {
    held.keys
        .names()
        .into_iter()
        .map(|name| {
            let key = held.keys.get(&name).expect("reads").expect("held");
            (name, spelled(&key))
        })
        .collect()
}

/// The reading the memoised one must equal: every print decoded at once, the history the largest
/// down-set of the objects, every conversation of it read and folded, and what it leaves out.
fn plain(
    prints: &BTreeMap<Hash, Vec<u8>>,
    keys: &KeyRing,
) -> (Result<Vec<Stored>, NotAConversation>, Vec<Excluded>) {
    let dag = Dag::<LineEvent>::from_prints(
        &LineVocabulary,
        prints
            .iter()
            .map(|(name, bytes)| (name.clone(), Ok(bytes.clone()))),
    );
    let nodes = dag.interior().nodes().expect("a down-set reads");
    let stored = transcript::transcripts(&nodes, &|name: &KeyName| keys.key(name)).map(|read| {
        read.into_values()
            .rev()
            .map(Transcript::into_stored)
            .collect()
    });
    (stored, dag.excluded())
}

fn memoised(lines: &mut HeldLines) -> (Result<Vec<Stored>, NotAConversation>, Vec<Excluded>) {
    let (read, _) = lines.read();
    let stored = read.map(|all| all.iter().map(|one| (**one).clone()).collect());
    (stored, lines.excluded())
}

/// What arrives at one step: a run of the prints, a run of the keys, a print let go, or a key.
#[derive(Debug, Clone)]
enum Arrival {
    Prints(usize),
    Keys(usize),
    Drop(prop::sample::Index),
    DropKey(prop::sample::Index),
}

fn arrival() -> impl Strategy<Value = Arrival> {
    prop_oneof![
        4 => (1usize..6).prop_map(Arrival::Prints),
        2 => (1usize..12).prop_map(Arrival::Keys),
        1 => any::<prop::sample::Index>().prop_map(Arrival::Drop),
        1 => any::<prop::sample::Index>().prop_map(Arrival::DropKey),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    /// THE MEMO LAW: after every arrival, in any order, of prints (one of them forged, so it reads
    /// as unread), of keys (one naming slots of two conversations), and of prints and keys let
    /// go, the memoised reading and what it leaves out are
    /// the plain reading's of what is held.
    #[test]
    fn the_memoised_reading_is_the_plain_one(
        script in proptest::collection::vec(step(), 1..24),
        order in any::<u64>(),
        arrivals in proptest::collection::vec(arrival(), 1..40),
        forged in any::<prop::sample::Index>(),
    ) {
        let held = written(&script);
        let mut all = prints(&held);
        let mut all_keys = keys(&held);
        let mut rng = rand_order(order);
        shuffle(&mut all, &mut rng);
        shuffle(&mut all_keys, &mut rng);
        let at = forged.index(all.len());
        all[at].1.push(b' ');
        let (mut prints_left, mut keys_left) = (all.clone(), all_keys);
        let mut holding: BTreeMap<Hash, Vec<u8>> = BTreeMap::new();
        let mut ring = KeyRing::default();
        let mut given: BTreeMap<KeyName, String> = BTreeMap::new();
        let mut lines = HeldLines::new();
        for arrival in arrivals {
            match arrival {
                Arrival::Prints(n) => {
                    let run: Vec<_> = prints_left.drain(..n.min(prints_left.len())).collect();
                    holding.extend(run.iter().cloned());
                    lines.hold(run, &[]);
                }
                Arrival::Keys(n) => {
                    let run: BTreeMap<KeyName, String> =
                        keys_left.drain(..n.min(keys_left.len())).collect();
                    ring.change(&run, &[]);
                    lines.keys(&run, &[]);
                    given.extend(run);
                }
                Arrival::DropKey(which) => {
                    if given.is_empty() {
                        continue;
                    }
                    let name = given.keys().nth(which.index(given.len())).cloned().expect("given");
                    let bytes = given.remove(&name).expect("given");
                    keys_left.push((name.clone(), bytes));
                    ring.change(&BTreeMap::new(), std::slice::from_ref(&name));
                    lines.keys(&BTreeMap::new(), &[name]);
                }
                Arrival::Drop(which) => {
                    if holding.is_empty() {
                        continue;
                    }
                    let name = holding.keys().nth(which.index(holding.len())).cloned().expect("held");
                    let bytes = holding.remove(&name).expect("held");
                    prints_left.push((name.clone(), bytes));
                    lines.hold(Vec::new(), &[name]);
                }
            }
            prop_assert_eq!(memoised(&mut lines), plain(&holding, &ring));
        }
    }
}

/// A seeded order, so a failure replays.
fn rand_order(seed: u64) -> impl FnMut(usize) -> usize {
    let mut state = seed | 1;
    move |bound| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        usize::try_from(state % u64::try_from(bound).expect("a bound")).expect("below the bound")
    }
}

fn shuffle<T>(items: &mut [T], pick: &mut impl FnMut(usize) -> usize) {
    for at in (1..items.len()).rev() {
        items.swap(at, pick(at + 1));
    }
}

/// THE COST LAW: once a store is read, a new line computes its own leaf, its conversation and
/// the store, and finds every other conversation, and every other line of its own, in the cache.
#[test]
fn a_new_line_costs_its_leaf_its_conversation_and_the_store() {
    let script: Vec<Step> = (0..30)
        .map(|n| Step::Said {
            conversation: n % 3,
            model: n % 2 == 0,
            echoes: false,
        })
        .collect();
    let held = written(&script);
    let mut lines = HeldLines::new();
    let given: BTreeMap<KeyName, String> = keys(&held).into_iter().collect();
    lines.keys(&given, &[]);
    lines.hold(prints(&held), &[]);
    let (read, _) = lines.read();
    assert!(read.is_ok());
    let before: BTreeSet<Hash> = prints(&held).into_iter().map(|(name, _)| name).collect();
    let in_first = transcript::read(held.conversations(), &id(0))
        .expect("read")
        .lines()
        .len();
    transcript::push(
        held.conversations(),
        &id(0),
        &Entry::Said {
            at: at(99),
            turn: Turn::Person {
                text: "one more".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: Some("ada".into()),
        },
    )
    .expect("said");
    let new: Vec<_> = prints(&held)
        .into_iter()
        .filter(|(name, _)| !before.contains(name))
        .collect();
    let given: BTreeMap<KeyName, String> = keys(&held).into_iter().collect();
    lines.keys(&given, &[]);
    lines.hold(new, &[]);
    let (read, report) = lines.read();
    assert!(read.is_ok());
    let count = |computed, hits| Counts {
        hits,
        computed,
        findings: 0,
    };
    assert_eq!(
        report.by_depth(),
        [count(1, 0), count(1, 2), count(1, in_first)],
        "the store, one conversation of three, one line of {}",
        in_first + 1
    );
}
