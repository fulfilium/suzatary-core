//! The memoised reading of the lines is the plain one, whatever order the prints and their keys
//! arrive in and whatever is let go, and a new line costs its own leaf, its conversation and the
//! store, every other node being one hit; and the stream, drawn round by round from only the lines
//! its rounds read, is the stream of the conversations read whole.

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
use suzatary_core::secretary::held_lines::{Admitted, HeldLines, decodes, hashes};
use suzatary_core::secretary::inbox::ProposalId;
use suzatary_core::secretary::key_ring::{KeyRing, spelled};
use suzatary_core::secretary::line_event::{LineEvent, LineVocabulary};
use suzatary_core::secretary::line_shape::LineShape;
use suzatary_core::secretary::locked_text::KeyName;
use suzatary_core::secretary::locked_text::opens;
use suzatary_core::secretary::outlines::{Outlines, Parsed};
use suzatary_core::secretary::readings::Dress;
use suzatary_core::secretary::rescrub;
use suzatary_core::secretary::scrubbed_under::ScrubbedUnder;
use suzatary_core::secretary::store::{
    ConversationId, Entry, Kind, Namer, NotAConversation, Stored, Title,
};
use suzatary_core::secretary::stream;
use suzatary_core::secretary::topics::reading::Reading;
use suzatary_core::secretary::topics::targets::rounds;
use suzatary_core::secretary::transcript::{self, Transcript};
use suzatary_core::secretary::words::Words;
use suzatary_core::store::Prodrome as _;
use suzatary_core::store::Reads as _;
use suzatary_core::store::{Doc as _, Gets as _};
use suzatary_core::wire;

/// One step of a script: a turn said in a conversation, an erasure of some of its lines, a
/// rename, a stop, a rewind of the last `back` turns, or a proposal. A turn that `echoes` draws its
/// keys as the last conversation opened drew its first, so one key names slots of two
/// conversations.
#[derive(Debug, Clone)]
enum Step {
    Said {
        conversation: usize,
        model: bool,
        echoes: bool,
    },
    Erase {
        conversation: usize,
        lines: Vec<prop::sample::Index>,
    },
    Rename {
        conversation: usize,
    },
    Stop {
        conversation: usize,
    },
    Rewind {
        conversation: usize,
        back: usize,
    },
    Propose {
        conversation: usize,
    },
}

impl Step {
    fn conversation(&self) -> usize {
        match self {
            Step::Said { conversation, .. }
            | Step::Erase { conversation, .. }
            | Step::Rename { conversation }
            | Step::Stop { conversation }
            | Step::Rewind { conversation, .. }
            | Step::Propose { conversation } => *conversation,
        }
    }
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        8 => (0usize..3, any::<bool>(), proptest::bool::weighted(0.2))
            .prop_map(|(conversation, model, echoes)| Step::Said { conversation, model, echoes }),
        2 => (0usize..3, proptest::collection::vec(any::<prop::sample::Index>(), 1..3))
            .prop_map(|(conversation, lines)| Step::Erase { conversation, lines }),
        1 => (0usize..3).prop_map(|conversation| Step::Rename { conversation }),
        1 => (0usize..3).prop_map(|conversation| Step::Stop { conversation }),
        1 => (0usize..3, 0usize..3).prop_map(|(conversation, back)| Step::Rewind { conversation, back }),
        1 => (0usize..3).prop_map(|conversation| Step::Propose { conversation }),
    ]
}

fn at(n: u32) -> Datetime {
    Datetime::new(2026, 10, 1, 9, n / 60 % 60, n % 60, 0).expect("an instant")
}

fn id(n: usize) -> ConversationId {
    ConversationId::new(format!("2026-10-01-{}", n + 1)).expect("a slug")
}

/// Keys drawn one count past the last, from a count a script may set back. Its last byte is marked
/// and the byte eight before it, which prints the same byte of the count, is not, so no key equals
/// one the store's own entropy (`Counting`) draws for the stale lines and the reseals: a reseal
/// holding another line's key would make one key two slots of one conversation, which no real draw
/// does.
#[derive(Default)]
struct Rewound(AtomicU64);

impl Entropy for Rewound {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        let count = self.0.fetch_add(1, Ordering::Relaxed);
        let len = bytes.len();
        for (at, byte) in bytes.iter_mut().enumerate() {
            *byte = count.to_be_bytes()[at % 8] ^ u8::try_from(at % 251).unwrap_or(0);
            if len > 8 && at == len - 1 {
                *byte ^= 0xA5;
            }
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
        let conversation = step.conversation();
        if let (Step::Said { echoes: true, .. }, Some(count)) = (step, opening) {
            entropy.0.store(count, Ordering::Relaxed);
        }
        if started.insert(conversation) {
            opening = Some(entropy.0.load(Ordering::Relaxed));
            transcript::start(
                store,
                &id(conversation),
                &Entry::Opened {
                    at: at(n),
                    title: Words::kept(format!("Topic {conversation}")),
                    model: "a-cli/small".to_owned(),
                    kind: Kind::Topic,
                },
            )
            .expect("started");
        }
        act(store, step, n);
    }
    held
}

/// Step `step`, the `n`th, written into `store` through the box's own writes.
fn act(store: Conversations<'_>, step: &Step, n: u32) {
    let id = id(step.conversation());
    let push = |entry: Entry| transcript::push(store, &id, &entry).expect("pushed");
    match step {
        Step::Said { model, .. } => {
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
            push(Entry::Said {
                at: at(n),
                turn,
                by,
            });
        }
        Step::Erase { lines, .. } => {
            let read = transcript::read(store, &id).expect("read");
            let keys: BTreeSet<KeyName> = lines
                .iter()
                .filter_map(|line| read.lines().get(line.index(read.lines().len())))
                .flat_map(|written| {
                    // An erasure names a slot by its first key, so erasing a reseal erases its slot.
                    if let LineShape::Resealed { of, .. } = written.line.shape() {
                        vec![of.clone()]
                    } else {
                        written.line.keys().cloned().collect()
                    }
                })
                .collect();
            if !keys.is_empty() {
                let by = Erasure::Asked {
                    by: Actor::new("ada").expect("a name"),
                };
                transcript::erase(store, &id, keys, by, at(n)).expect("erased");
            }
        }
        Step::Rename { .. } => push(Entry::Titled {
            at: at(n),
            title: Words::kept(Title::new(format!("Renamed {n}")).expect("a title")),
            by: Namer::Person {
                actor: "ada".to_owned(),
            },
        }),
        Step::Stop { .. } => push(Entry::Stopped {
            at: at(n),
            by: "ada".to_owned(),
        }),
        Step::Rewind { back, .. } => {
            let turns = transcript::read(store, &id)
                .expect("read")
                .stored()
                .turns
                .len();
            push(Entry::Rewound {
                to: turns.saturating_sub(*back),
                at: at(n),
                by: "ada".to_owned(),
            });
        }
        Step::Propose { .. } => push(Entry::Proposed {
            at: at(n),
            object: ProposalId::new(format!("p{n:032x}")).expect("an id"),
            revises: None,
        }),
    }
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
/// down-set of the objects, every conversation of it read and folded, and what it leaves out, less
/// the names the skeleton (`stored`) holds that this replica did not fetch.
fn plain(
    prints: &BTreeMap<Hash, Vec<u8>>,
    keys: &KeyRing,
    stored: &BTreeSet<Hash>,
) -> (Result<Vec<Stored>, NotAConversation>, Vec<Excluded>) {
    let dag = Dag::<LineEvent>::from_prints(
        &LineVocabulary,
        prints
            .iter()
            .map(|(name, bytes)| (name.clone(), Ok(bytes.clone()))),
    );
    let nodes = dag.interior().nodes().expect("a down-set reads");
    let read = transcript::transcripts(&nodes, &|name: &KeyName| keys.key(name)).map(|read| {
        read.into_values()
            .rev()
            .map(Transcript::into_stored)
            .collect()
    });
    let held = dag
        .excluded()
        .into_iter()
        .filter(|excluded| excluded.why.is_some() || !stored.contains(&excluded.name))
        .collect();
    (read, held)
}

fn memoised(
    lines: &mut HeldLines,
    stored: &BTreeSet<Hash>,
) -> (Result<Vec<Stored>, NotAConversation>, Vec<Excluded>) {
    let (read, _) = lines.read();
    let read = read.map(|all| all.iter().map(|one| (**one).clone()).collect());
    (read, lines.excluded(|name| stored.contains(name)))
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
    /// go, the memoised reading and what it leaves out are the plain reading's of what is held,
    /// less the names a skeleton holds that were not fetched. The skeleton lacks one object, a
    /// hole that what rests on it reports while that object is not held.
    #[test]
    fn the_memoised_reading_is_the_plain_one(
        script in proptest::collection::vec(step(), 1..24),
        order in any::<u64>(),
        arrivals in proptest::collection::vec(arrival(), 1..40),
        forged in any::<prop::sample::Index>(),
        hole in any::<prop::sample::Index>(),
    ) {
        let held = written(&script);
        let mut all = prints(&held);
        let mut all_keys = keys(&held);
        let mut rng = rand_order(order);
        shuffle(&mut all, &mut rng);
        shuffle(&mut all_keys, &mut rng);
        let at = forged.index(all.len());
        all[at].1.push(b' ');
        let missing = all[hole.index(all.len())].0.clone();
        let skeleton: BTreeSet<Hash> =
            all.iter().map(|(name, _)| name.clone()).filter(|name| *name != missing).collect();
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
            prop_assert_eq!(memoised(&mut lines, &skeleton), plain(&holding, &ring, &skeleton));
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

/// The lines a store holds as the box's skeleton sends them, by conversation.
fn skeleton(held: &InMemory) -> Outlines {
    let mut skeleton = Outlines::default();
    skeleton.hold(
        held.lines
            .objects()
            .expect("objects")
            .iter()
            .map(|(name, object)| {
                let sent =
                    serde_json::to_string(&suzatary_core::routes::skeleton_node(name, object))
                        .expect("prints");
                Parsed::of(serde_json::from_str(&sent).expect("reads back")).expect("a node")
            })
            .collect(),
        &[],
    );
    skeleton
}

/// The stream as the conversations read whole draw it: every live round of every conversation,
/// the newest `pages` pages of them, each drawn from its conversation's fold.
fn whole(all: &[std::sync::Arc<Stored>], pages: usize) -> wire::StreamPayload {
    let mut shown: Vec<_> = all
        .iter()
        .flat_map(|stored| {
            rounds(stored).into_iter().map(move |round| {
                (
                    round.opening.1.at,
                    &stored.id,
                    round.opening.0,
                    stored,
                    round,
                )
            })
        })
        .collect();
    shown.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
    let total = shown.len();
    wire::StreamPayload {
        rounds: shown
            .into_iter()
            .skip(total.saturating_sub(pages * stream::PAGE))
            .map(|(_, _, _, stored, round)| {
                stream::row(&stored.title, &round, &Reading::default(), &Dress::BARE)
            })
            .collect(),
        total,
    }
}

/// Every line of `held` whose words were sealed under older rules sealed again, as
/// `migrate-scrub` does, after `stale` writes its turns and titles under the rules from before lines
/// recorded them; then, if `swept`, every key a reseal replaced destroyed; then each erasure of
/// `after`, whose lines may be first writes or reseals.
fn resealed(held: &InMemory, stale: &[usize], swept: bool, after: &[Step]) {
    for (n, conversation) in (500u32..).zip(stale) {
        let id = id(*conversation);
        let Ok(read) = transcript::read(held.conversations(), &id) else {
            continue;
        };
        let entry = if n % 2 == 0 {
            Entry::Titled {
                at: at(n),
                title: Words::kept(Title::new(format!("stale name {n} s3cret")).expect("a title")),
                by: Namer::Person {
                    actor: "ada".to_owned(),
                },
            }
        } else {
            Entry::Said {
                at: at(n),
                turn: Turn::Person {
                    text: format!("stale question {n} at https://example.test/r?token=s3cret"),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: Some("ada".into()),
            }
        };
        stale_line(held, &id, read.lines().len(), &entry);
    }
    rescrub::reseal(held.conversations(), at(900)).expect("reseals");
    if swept {
        transcript::sweep(held.conversations()).expect("sweeps");
    }
    for (n, step) in (950u32..).zip(after) {
        if transcript::read(held.conversations(), &id(step.conversation())).is_ok() {
            act(held.conversations(), step, n);
        }
    }
}

/// Line `count` of conversation `id`, `entry`, sealed under the rules from before lines recorded
/// them.
fn stale_line(held: &InMemory, id: &ConversationId, count: usize, entry: &Entry) {
    let (line, keys) = LineEvent::written_under(
        &ScrubbedUnder::Unversioned,
        id.clone(),
        Actor::new("assistant").expect("a name"),
        u32::try_from(count).expect("a count"),
        entry,
        &held.entropy,
    )
    .expect("a line");
    for key in &keys {
        held.keys.set(&key.name(), key).expect("kept");
    }
    held.lines.append(line).expect("appended");
}

/// What one reading cost: prints decoded, prints hashed, and slots decrypted.
fn costs() -> (usize, usize, usize) {
    (decodes(), hashes(), opens())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    /// THE ROUND LAW: the stream chosen by the skeleton's outlines and drawn round by round, from a
    /// reader holding every line or from one holding only the lines the drawn rounds read, is the
    /// stream of the conversations read whole, whatever was renamed, stopped, rewound, proposed,
    /// erased (before or after a reseal, its first write or the reseal), sealed again or swept. Holding
    /// those lines decodes exactly their prints and hashes none of them, opening their words
    /// decrypts no slot outside them, and reading the stream again decodes and decrypts nothing.
    #[test]
    fn the_stream_reads_only_the_rounds_it_draws(
        script in proptest::collection::vec(step(), 1..90),
        stale in proptest::collection::vec(0usize..3, 0..4),
        swept in any::<bool>(),
        after in proptest::collection::vec(
            (0usize..3, proptest::collection::vec(any::<prop::sample::Index>(), 1..3))
                .prop_map(|(conversation, lines)| Step::Erase { conversation, lines }),
            0..3,
        ),
        pages in 1usize..3,
    ) {
        let held = written(&script);
        resealed(&held, &stale, swept, &after);
        let given: BTreeMap<KeyName, String> = keys(&held).into_iter().collect();
        let topics = Reading::default();
        let chosen = BTreeSet::new();

        let mut all = HeldLines::new();
        all.keys(&given, &[]);
        all.hold(prints(&held), &[]);
        let read_whole = whole(&all.read().0.expect("reads"), pages);

        let skeleton = skeleton(&held);
        let objects = held.lines.objects().expect("objects");
        let opened = transcript::opened(&objects, &held.conversations().reads().slots());
        let eager = stream::stream(skeleton.outlines(), stream::every(&opened), &topics, &chosen, pages, &Dress::BARE)
            .expect("reads");
        prop_assert_eq!(&eager, &read_whole);

        let names = stream::choose(skeleton.outlines(), &topics, &chosen, pages).expect("chooses").objects();
        let slots: usize = objects
            .iter()
            .filter(|(name, _)| names.contains(name))
            .filter_map(|(_, object)| object.event())
            .map(|line| line.keys().count())
            .sum();
        let mut lazy = HeldLines::new();
        lazy.keys(&given, &[]);
        let before = costs();
        lazy.hold_admitted(
            prints(&held)
                .into_iter()
                .filter(|(name, _)| names.contains(name))
                .map(|(name, bytes)| Admitted::vouched(name, bytes))
                .collect(),
            &[],
        );
        let draw = |lazy: &mut HeldLines| {
            stream::stream(skeleton.outlines(), |id, turn, objects| lazy.round(id, turn, objects), &topics, &chosen, pages, &Dress::BARE)
                .expect("reads")
        };
        let first = draw(&mut lazy);
        let after = costs();
        prop_assert_eq!(&first, &eager);
        prop_assert_eq!(after.0 - before.0, names.len(), "decodes exactly the drawn rounds' lines");
        prop_assert_eq!(after.1 - before.1, 0, "hashes none of them again");
        prop_assert!(after.2 - before.2 <= slots, "decrypts {} slots, of {} in the drawn rounds' lines", after.2 - before.2, slots);
        prop_assert_eq!(draw(&mut lazy), eager);
        prop_assert_eq!(costs(), after, "a second stream decodes and decrypts nothing");
    }
}

/// A store at the live shape: 96 conversations of 49 turns, then one of 2,500 turns, three of five
/// turns written among its last twenty, every turn sealed under the rules from before lines
/// recorded them, then sealed again and its old key destroyed. Answers the store and the big
/// conversation.
fn live_shape() -> (InMemory, ConversationId) {
    let held = InMemory::default();
    let mut n = 0u32;
    let mut said = |id: &ConversationId, count: usize, model: bool| {
        n += 1;
        let at = Datetime::new(
            2026,
            10,
            1 + n / 86_400,
            n / 3600 % 24,
            n / 60 % 60,
            n % 60,
            0,
        )
        .expect("an instant");
        let entry = if count == 0 {
            Entry::Opened {
                at,
                title: Words::kept(format!("Topic {id}")),
                model: "a-cli/small".to_owned(),
                kind: Kind::Topic,
            }
        } else if model {
            Entry::Said {
                at,
                turn: Turn::Model {
                    text: format!("Answer {n}, about https://example.test/r?token=s3cret."),
                    calls: Vec::new(),
                },
                by: None,
            }
        } else {
            Entry::Said {
                at,
                turn: Turn::Person {
                    text: format!("question {n}"),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: Some("ada".into()),
            }
        };
        stale_line(&held, id, count, &entry);
    };
    let big = id(99);
    let mut lines = BTreeMap::<ConversationId, usize>::new();
    let mut write =
        |id: &ConversationId, turns: usize, said: &mut dyn FnMut(&ConversationId, usize, bool)| {
            let count = lines.entry(id.clone()).or_default();
            for _ in 0..turns {
                said(id, *count, count.is_multiple_of(2));
                *count += 1;
            }
        };
    for conversation in 0..96 {
        write(&id(conversation), 49, &mut said);
    }
    write(&big, 2_480, &mut said);
    for conversation in 96..99 {
        write(&id(conversation), 5, &mut said);
    }
    write(&big, 20, &mut said);
    rescrub::run(held.conversations(), at(3599)).expect("reseals and sweeps");
    (held, big)
}

/// THE STREAM'S COST AT THE LIVE SHAPE: about 14,800 lines over 100 conversations, every turn
/// sealed again once and its replaced key destroyed, and one conversation of about 5,000 lines
/// holding most of the newest page. The first stream holds, decodes and decrypts only the lines
/// of the 30 rounds it draws and their reseals, and the second costs nothing.
#[test]
fn the_first_stream_at_the_live_shape_reads_only_its_rounds() {
    let (held, big) = live_shape();
    let objects = held.lines.objects().expect("objects");
    assert!(
        (14_000..15_500).contains(&objects.len()),
        "{} objects",
        objects.len()
    );

    let skeleton = skeleton(&held);
    let choice = stream::choose(
        skeleton.outlines(),
        &Reading::default(),
        &BTreeSet::new(),
        1,
    )
    .expect("chooses");
    assert_eq!(choice.drawn.len(), stream::PAGE);
    assert!(
        choice
            .drawn
            .iter()
            .filter(|drawn| *drawn.conversation == big)
            .count()
            > stream::PAGE / 2
    );
    let names = choice.objects();
    let slots: usize = objects
        .iter()
        .filter(|(name, _)| names.contains(name))
        .filter_map(|(_, object)| object.event())
        .map(|line| line.keys().count())
        .sum();
    let mut lazy = HeldLines::new();
    lazy.keys(&keys(&held).into_iter().collect(), &[]);
    let before = costs();
    lazy.hold_admitted(
        prints(&held)
            .into_iter()
            .filter(|(name, _)| names.contains(name))
            .map(|(name, bytes)| Admitted::vouched(name, bytes))
            .collect(),
        &[],
    );
    let draw = |lazy: &mut HeldLines| {
        stream::stream(
            skeleton.outlines(),
            |id, turn, objects| lazy.round(id, turn, objects),
            &Reading::default(),
            &BTreeSet::new(),
            1,
            &Dress::BARE,
        )
        .expect("reads")
    };
    let first = draw(&mut lazy);
    let after = costs();
    assert_eq!(first.rounds.len(), stream::PAGE);
    assert_eq!(
        (after.0 - before.0, after.1 - before.1),
        (names.len(), 0),
        "gets and hashes"
    );
    assert!(
        after.2 - before.2 <= slots,
        "decrypts {} of {slots}",
        after.2 - before.2
    );
    assert!(
        names.len() < 200,
        "{} of {} objects",
        names.len(),
        objects.len()
    );
    assert_eq!(draw(&mut lazy), first);
    assert_eq!(costs(), after, "the second stream costs nothing");
}

/// A print of a drawn round that does not decode leaves that round out of the stream, and the
/// stream still draws every other round as the whole store draws it.
#[test]
fn a_print_that_does_not_decode_leaves_out_its_round_and_draws_the_rest() {
    let script: Vec<Step> = (0..12)
        .map(|k| Step::Said {
            conversation: 0,
            model: k % 2 == 1,
            echoes: false,
        })
        .collect();
    let held = written(&script);
    let given: BTreeMap<KeyName, String> = keys(&held).into_iter().collect();
    let skeleton = skeleton(&held);
    let read = |prints: Vec<(Hash, Vec<u8>)>| {
        let mut lines = HeldLines::new();
        lines.keys(&given, &[]);
        lines.hold(prints, &[]);
        stream::stream(
            skeleton.outlines(),
            |id, turn, objects| lines.round(id, turn, objects),
            &Reading::default(),
            &BTreeSet::new(),
            1,
            &Dress::BARE,
        )
    };
    let whole = read(prints(&held)).expect("reads");
    // The ninth line of twelve, a question that opens a round.
    let ninth = transcript::read(held.conversations(), &id(0))
        .expect("read")
        .lines()[9]
        .name
        .clone();
    let forged: Vec<_> = prints(&held)
        .into_iter()
        .map(|(name, mut bytes)| {
            if name == ninth {
                bytes.push(b' ');
            }
            (name, bytes)
        })
        .collect();
    let part = read(forged).expect("still reads");
    assert_eq!(part.total, whole.total, "the count is the skeleton's");
    assert_eq!(part.rounds.len(), whole.rounds.len() - 1);
    assert!(part.rounds.iter().all(|round| whole.rounds.contains(round)));
}
