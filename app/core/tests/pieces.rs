//! A store's pull by pieces (`routes::store_objects_since`, `routes::lines_since`): asked again
//! and again from what the last piece left, while the store moves between the asks, each piece
//! joined to what the caller holds holds everything it rests on (its parents and the genesis of
//! its prodrome), forks and the merges after them included; the pieces together are what one
//! answer of the whole would send; no piece carries a key an erasure anywhere in the store names;
//! and every piece names every such key.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::{Actor, Envelope, Hash, parents_of};
use prodrome_core::literal::Datetime;
use proptest::prelude::*;

use suzatary_core::capability::Turn;
use suzatary_core::frontier::{Digest, Frontiers, Since};
use suzatary_core::routes::{self, Books, Cut};
use suzatary_core::secretary::conversations::InMemory;
use suzatary_core::secretary::erasure::Erasure;
use suzatary_core::secretary::line_event::LineEvent;
use suzatary_core::secretary::store::{ConversationId, Entry, Kind};
use suzatary_core::secretary::transcript;
use suzatary_core::secretary::words::Words;
use suzatary_core::store::Reads as _;
use suzatary_core::store::{Doc as _, Prodrome as _};

fn at(n: u32) -> Datetime {
    Datetime::new(2026, 9, 30, 9, n / 60, n % 60, 0).expect("an instant")
}

fn now() -> Datetime {
    Datetime::new(2026, 9, 30, 12, 0, 0, 0).expect("an instant")
}

fn id(n: usize) -> ConversationId {
    ConversationId::new(format!("2026-09-30-{n}")).expect("a slug")
}

/// One write to one of a few conversations: a turn of some length, the erasure of the words of
/// its latest line that has any, or a fork: two replicas each writing a turn from the same lines,
/// joined, so the next line of that conversation rests on both.
#[derive(Debug, Clone)]
enum Write {
    Said { conversation: usize, words: usize },
    Erased { conversation: usize },
    Fork { conversation: usize, words: usize },
}

fn write() -> impl Strategy<Value = Write> {
    prop_oneof![
        4 => (0..3usize, 1..600usize).prop_map(|(conversation, words)| Write::Said { conversation, words }),
        1 => (0..3usize).prop_map(|conversation| Write::Erased { conversation }),
        1 => (0..3usize, 1..600usize).prop_map(|(conversation, words)| Write::Fork { conversation, words }),
    ]
}

/// A store the writes move, as the box writes it: every line a `Change` over its conversation's
/// heads, its keys kept first. An erasure's keys are kept, as a sweep that could not destroy them
/// leaves them, so only the pull's own reading keeps them out.
struct Writer {
    lines: InMemory,
    written: BTreeMap<usize, u32>,
    second: u32,
}

impl Writer {
    fn new() -> Self {
        Self {
            lines: InMemory::default(),
            written: BTreeMap::new(),
            second: 0,
        }
    }

    fn line(&mut self, conversation: usize, entry: &Entry) -> LineEvent {
        let held = self.written.entry(conversation).or_insert(0);
        let (line, keys) = LineEvent::write(
            id(conversation),
            Actor::new("assistant").expect("an actor"),
            *held,
            entry,
            &self.lines.entropy,
        )
        .expect("a line");
        *held += 1;
        for key in &keys {
            self.lines.keys.set(&key.name(), key).expect("kept");
        }
        line
    }

    fn said(&mut self, conversation: usize, words: usize) -> Entry {
        self.second += 1;
        if !self.written.contains_key(&conversation) {
            let opened = Entry::Opened {
                at: at(self.second),
                title: Words::kept(format!("Topic {conversation}")),
                model: "a-cli/small".to_owned(),
                kind: Kind::Topic,
            };
            let line = self.line(conversation, &opened);
            self.lines.lines.append(line).expect("opened");
        }
        let turn = Turn::Person {
            text: "a".repeat(words),
            references: Vec::new(),
            attachments: Vec::new(),
        };
        Entry::Said {
            at: at(self.second),
            turn,
            by: None,
        }
    }

    fn apply(&mut self, write: &Write) {
        match write {
            Write::Said {
                conversation,
                words,
            } => {
                let said = self.said(*conversation, *words);
                let line = self.line(*conversation, &said);
                self.lines.lines.append(line).expect("said");
            }
            Write::Fork {
                conversation,
                words,
            } => {
                let said = self.said(*conversation, *words);
                // The other replica holds what this one does, and each writes a line from it.
                let other = suzatary_core::store::InMemory::<LineEvent>::default();
                let held: Vec<(Hash, String)> = self
                    .lines
                    .lines
                    .objects()
                    .expect("reads")
                    .iter()
                    .map(|(name, object)| {
                        (
                            name.clone(),
                            prodrome_core::event::canonical_envelope(object),
                        )
                    })
                    .collect();
                other.accept(&held).expect("copied");
                let theirs = self.line(*conversation, &said);
                self.written.entry(*conversation).and_modify(|n| *n -= 1);
                let ours = self.line(*conversation, &said);
                other.append(theirs).expect("theirs");
                self.lines.lines.append(ours).expect("ours");
                let forked: Vec<(Hash, String)> = other
                    .objects()
                    .expect("reads")
                    .iter()
                    .map(|(name, object)| {
                        (
                            name.clone(),
                            prodrome_core::event::canonical_envelope(object),
                        )
                    })
                    .collect();
                self.lines.lines.accept(&forked).expect("joined");
            }
            Write::Erased { conversation } => {
                let objects = self.lines.lines.objects().expect("reads");
                let Some(keys) = objects
                    .iter()
                    .rev()
                    .filter_map(|(_, object)| object.event())
                    .filter(|line| *line.conversation() == id(*conversation))
                    .map(|line| line.keys().cloned().collect::<BTreeSet<_>>())
                    .find(|keys| !keys.is_empty())
                else {
                    return;
                };
                self.second += 1;
                let erased = Entry::Erased {
                    at: at(self.second),
                    keys,
                    erasure: Erasure::Asked {
                        by: Actor::new("ada").expect("an actor"),
                    },
                };
                let line = self.line(*conversation, &erased);
                self.lines.lines.append(line).expect("erased");
            }
        }
    }
}

/// Everything an object rests on, as named in it: its parents, and a change's genesis.
fn rests_on(object: &Envelope<LineEvent>) -> Vec<Hash> {
    let mut on = parents_of(object);
    if let Envelope::Change(change) = object {
        on.push(change.genesis.clone());
    }
    on
}

/// The frontier of `held`, a down-set of `objects`, recorded as an answer records one, as the
/// `since` that names it.
fn since_of(
    frontiers: &Frontiers,
    objects: &[(Hash, Envelope<LineEvent>)],
    held: &BTreeSet<Hash>,
) -> Since {
    let below: BTreeSet<Hash> = objects
        .iter()
        .filter(|(name, _)| held.contains(name))
        .flat_map(|(_, object)| parents_of(object))
        .collect();
    let heads = held.difference(&below).cloned().collect();
    Since::Frontier(frontiers.record("store/lines", &heads).expect("recorded"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Pieces asked one after another from what the last left, the store moving between them:
    /// each holds nothing already held, fits the budget unless it is one object, holds with what
    /// came before everything it rests on, carries no key an erasure in the store names, and
    /// names every such key; the pull ends holding the store as it is then, and, where nothing
    /// moved, the pieces are the whole answer, keys included.
    #[test]
    fn law_the_pieces_union_is_the_whole_and_each_is_ancestor_closed(
        script in prop::collection::vec(write(), 1..24),
        moves in prop::collection::vec(prop::option::of(write()), 0..6),
        held_first in 0..64usize,
        budget in 1..3000usize,
    ) {
        let mut store = Writer::new();
        for write in &script {
            store.apply(write);
        }
        let (epochs, frontiers) = (suzatary_core::epoch::Epochs::remembered(), Frontiers::remembered());
        let books = Books { epochs: &epochs, frontiers: &frontiers };
        let first = store.lines.lines.objects().expect("reads");
        // A replica holds a down-set: a prefix of the store's order.
        let mut held: BTreeSet<Hash> =
            first.iter().take(held_first).map(|(name, _)| name.clone()).collect();
        let mut since = since_of(&frontiers, &first, &held);
        let whole = routes::lines_since(store.lines.conversations(), &since, books, now(), Cut::Whole).expect("the whole");
        prop_assert_eq!(whole.rest, 0);
        let mut sent = BTreeSet::new();
        let mut keys = BTreeMap::new();
        let mut moved = false;
        for asked in 0usize.. {
            prop_assert!(asked <= first.len() + moves.len() * 4 + 1, "the pull ends");
            let objects = store.lines.lines.objects().expect("reads");
            let by_name: BTreeMap<&Hash, &Envelope<LineEvent>> = objects.iter().map(|(n, o)| (n, o)).collect();
            let erased: Vec<String> = transcript::erased(&objects).keys().map(|k| k.as_str().to_owned()).collect();
            let piece = routes::lines_since(store.lines.conversations(), &since, books, now(), Cut::Bytes(budget)).expect("a piece");
            prop_assert_eq!(piece.objects.since, suzatary_core::wire::SinceRead::Known);
            let lacked = objects.len() - held.len();
            prop_assert_eq!(piece.objects.objects.is_empty(), lacked == 0);
            prop_assert_eq!(piece.rest, lacked - piece.objects.objects.len());
            prop_assert_eq!(&piece.erased, &erased);
            let bytes: usize = piece.objects.objects.iter().map(|o| o.literal.len()).sum();
            prop_assert!(bytes <= budget || piece.objects.objects.len() == 1, "{} bytes over {}", bytes, budget);
            for object in &piece.objects.objects {
                let name = Hash::new(object.hash.clone()).expect("a name");
                prop_assert!(held.insert(name), "{} sent again", object.hash);
                sent.insert(object.hash.clone());
            }
            for object in &piece.objects.objects {
                let name = Hash::new(object.hash.clone()).expect("a name");
                for on in rests_on(by_name[&name]) {
                    prop_assert!(held.contains(&on), "{} came before {} it rests on", object.hash, on.as_str());
                }
            }
            for name in piece.keys.keys() {
                prop_assert!(!erased.contains(name), "the erased key {} was sent", name);
            }
            prop_assert_eq!(piece.objects.frontier.count, held.len(), "the frontier is what the replica holds");
            since = Since::Frontier(Digest::parse(&piece.objects.frontier.digest).expect("a digest"));
            keys.extend(piece.keys);
            if piece.rest == 0 {
                break;
            }
            if let Some(Some(write)) = moves.get(asked) {
                store.apply(write);
                moved = true;
            }
        }
        prop_assert_eq!(held.len(), store.lines.lines.objects().expect("reads").len());
        if !moved {
            let all: BTreeSet<String> = whole.objects.objects.iter().map(|o| o.hash.clone()).collect();
            prop_assert_eq!(sent, all);
            prop_assert_eq!(keys, whole.keys);
        }
    }
}

/// The writes make what the law needs: a fork leaves a conversation two heads, and the next line
/// rests on both.
#[test]
fn a_fork_then_a_line_is_a_merge() {
    let mut store = Writer::new();
    for write in [
        Write::Said {
            conversation: 0,
            words: 3,
        },
        Write::Fork {
            conversation: 0,
            words: 3,
        },
        Write::Said {
            conversation: 0,
            words: 3,
        },
    ] {
        store.apply(&write);
    }
    let objects = store.lines.lines.objects().expect("reads");
    let merges = objects
        .iter()
        .filter(|(_, object)| matches!(object, Envelope::Change(change) if change.deps.len() > 1))
        .count();
    assert_eq!(merges, 1);
}

/// The literals of `hashes`, asked of `lines` by hash as a tab would ask: in ascending order, in
/// answers of at most `query::MOST_HASHES` names and `routes::MOST_CONTENT_BYTES` bytes by the
/// sizes the skeleton gave, an object larger than that asked for alone.
fn fetched(
    lines: &dyn suzatary_core::store::Prodrome<LineEvent>,
    nodes: &BTreeMap<String, usize>,
) -> BTreeMap<String, String> {
    let mut asks: Vec<Vec<&str>> = vec![Vec::new()];
    let mut bytes = 0usize;
    for (hash, size) in nodes {
        let last = asks.last().map_or(0, Vec::len);
        if last == suzatary_core::query::MOST_HASHES
            || (last > 0 && bytes + size > routes::MOST_CONTENT_BYTES)
        {
            asks.push(Vec::new());
            bytes = 0;
        }
        bytes += size;
        if let Some(ask) = asks.last_mut() {
            ask.push(hash);
        }
    }
    asks.into_iter()
        .filter(|ask| !ask.is_empty())
        .flat_map(|ask| {
            let hashes = suzatary_core::query::Hashes::parse(Some(&ask.join(","))).expect("a set");
            routes::content(lines, &hashes).expect("held").objects
        })
        .map(|object| (object.hash, object.literal))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// The skeleton's pieces, asked one after another from what the last left while the store
    /// moves, and then each node's literal asked for by hash, rebuild what the objects' pull
    /// sends: each piece holds with what came before everything it rests on, as its parents say,
    /// and each node's parents and size are its literal's; the heads, prodromes and epoch are the
    /// pull's; the keys and the erasures are the pull's, no erased key among them. Where nothing
    /// moved, the objects come in the pull's own order, literal for literal.
    #[test]
    fn law_the_skeleton_and_the_content_by_hash_are_the_whole_pull(
        script in prop::collection::vec(write(), 1..24),
        moves in prop::collection::vec(prop::option::of(write()), 0..6),
        held_first in 0..64usize,
        budget in 200..4000usize,
    ) {
        let mut store = Writer::new();
        for write in &script {
            store.apply(write);
        }
        let (epochs, frontiers) = (suzatary_core::epoch::Epochs::remembered(), Frontiers::remembered());
        let books = Books { epochs: &epochs, frontiers: &frontiers };
        let first = store.lines.lines.objects().expect("reads");
        let mut held: BTreeSet<Hash> =
            first.iter().take(held_first).map(|(name, _)| name.clone()).collect();
        let since = since_of(&frontiers, &first, &held);
        let mut since_now = since.clone();
        let mut nodes: Vec<suzatary_core::wire::SkeletonNode> = Vec::new();
        let mut keys = BTreeMap::new();
        let mut moved = false;
        let mut asked = 0usize;
        let last = loop {
            let piece = routes::lines_skeleton(store.lines.conversations(), &since_now, books, now(), Cut::Bytes(budget)).expect("a piece");
            for node in &piece.nodes {
                for parent in &node.parents {
                    let parent = Hash::new(parent.clone()).expect("a name");
                    prop_assert!(held.contains(&parent), "{} came before {} it rests on", node.hash, parent.as_str());
                }
                prop_assert!(held.insert(Hash::new(node.hash.clone()).expect("a name")), "{} sent again", node.hash);
            }
            for name in piece.keys.keys() {
                prop_assert!(!piece.erased.contains(name), "the erased key {} was sent", name);
            }
            prop_assert_eq!(piece.frontier.count, held.len(), "the frontier is what the replica holds");
            since_now = Since::Frontier(Digest::parse(&piece.frontier.digest).expect("a digest"));
            keys.extend(piece.keys.clone());
            nodes.extend(piece.nodes.clone());
            if piece.rest == 0 {
                break piece;
            }
            if let Some(Some(write)) = moves.get(asked) {
                store.apply(write);
                moved = true;
            }
            asked += 1;
            prop_assert!(asked <= first.len() + moves.len() * 4 + 1, "the pull ends");
        };
        let whole = routes::lines_since(store.lines.conversations(), &since, books, now(), Cut::Whole).expect("the whole");
        prop_assert_eq!(&last.heads, &whole.objects.heads);
        prop_assert_eq!(&last.geneses, &whole.objects.geneses);
        prop_assert_eq!(last.epoch, whole.objects.epoch);
        prop_assert_eq!(&last.erased, &whole.erased);
        let sizes: BTreeMap<String, usize> = nodes.iter().map(|node| (node.hash.clone(), node.size)).collect();
        let literals = fetched(&store.lines.lines, &sizes);
        let objects = store.lines.lines.objects().expect("reads");
        let by_name: BTreeMap<&Hash, &Envelope<LineEvent>> = objects.iter().map(|(n, o)| (n, o)).collect();
        for node in &nodes {
            let name = Hash::new(node.hash.clone()).expect("a name");
            let literal = &literals[&node.hash];
            prop_assert_eq!(node.size, literal.len());
            prop_assert_eq!(literal, &prodrome_core::event::canonical_envelope(by_name[&name]));
            let mut parents: Vec<String> = rests_on(by_name[&name]).iter().map(|h| h.as_str().to_owned()).collect();
            parents.sort();
            parents.dedup();
            prop_assert_eq!(&node.parents, &parents);
        }
        let rebuilt: Vec<suzatary_core::wire::StoredObject> = nodes
            .iter()
            .map(|node| suzatary_core::wire::StoredObject { hash: node.hash.clone(), literal: literals[&node.hash].clone() })
            .collect();
        if moved {
            let sent: BTreeSet<&str> = rebuilt.iter().map(|o| o.hash.as_str()).collect();
            let all: BTreeSet<&str> = whole.objects.objects.iter().map(|o| o.hash.as_str()).collect();
            prop_assert_eq!(sent, all);
            for (name, key) in &whole.keys {
                prop_assert_eq!(keys.get(name), Some(key));
            }
        } else {
            prop_assert_eq!(&rebuilt, &whole.objects.objects);
            prop_assert_eq!(&keys, &whole.keys);
        }
    }
}

/// A content answer is a function of the hashes asked: the same question asked again after the
/// store moved, an erasure of those very words included, answers the same bytes.
#[test]
fn a_content_answer_never_changes() {
    let mut store = Writer::new();
    store.apply(&Write::Said {
        conversation: 0,
        words: 40,
    });
    let objects = store.lines.lines.objects().expect("reads");
    let asked = objects
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",");
    let hashes = suzatary_core::query::Hashes::parse(Some(&asked)).expect("a set");
    let before = routes::content(&store.lines.lines, &hashes).expect("held");
    for write in [
        Write::Said {
            conversation: 1,
            words: 9,
        },
        Write::Erased { conversation: 0 },
        Write::Fork {
            conversation: 0,
            words: 3,
        },
    ] {
        store.apply(&write);
    }
    let after = routes::content(&store.lines.lines, &hashes).expect("held");
    assert_eq!(
        serde_json::to_vec(&before).expect("prints"),
        serde_json::to_vec(&after).expect("prints")
    );
}

/// A hash another store holds is refused by this one in the very words a hash no store holds
/// is: a store's content route reads that store alone, and says nothing of the others.
#[test]
fn another_store_s_hash_is_refused_as_an_unknown_one_is() {
    let (mut ours, mut theirs) = (Writer::new(), Writer::new());
    ours.apply(&Write::Said {
        conversation: 0,
        words: 5,
    });
    theirs.apply(&Write::Said {
        conversation: 2,
        words: 7,
    });
    let elsewhere = theirs.lines.lines.tips().expect("reads");
    let elsewhere = elsewhere.first().expect("a head").as_str().to_owned();
    let nowhere = "0".repeat(64);
    let refused = |hash: &str| {
        let hashes = suzatary_core::query::Hashes::parse(Some(hash)).expect("a set");
        let refusal = routes::content(&ours.lines.lines, &hashes).expect_err("not ours");
        (refusal.status(), refusal.message().replace(hash, "<hash>"))
    };
    assert_eq!(refused(&elsewhere), refused(&nowhere));
    assert_eq!(
        refused(&nowhere).0,
        suzatary_core::refusal::Status::NotFound
    );
}

/// The bound holds: a set whose literals pass the bytes one answer carries is refused whole,
/// and one object is answered whatever its size.
#[test]
fn the_content_bound_holds() {
    let mut store = Writer::new();
    for conversation in 0..3 {
        store.apply(&Write::Said {
            conversation,
            words: 500,
        });
    }
    let objects = store.lines.lines.objects().expect("reads");
    let mut names: Vec<&str> = objects.iter().map(|(name, _)| name.as_str()).collect();
    names.sort_unstable();
    let total: usize = objects
        .iter()
        .map(|(_, object)| prodrome_core::event::canonical_envelope(object).len())
        .sum();
    assert!(
        total < routes::MOST_CONTENT_BYTES,
        "the store fits one answer"
    );
    let all = suzatary_core::query::Hashes::parse(Some(&names.join(","))).expect("a set");
    assert_eq!(
        routes::content(&store.lines.lines, &all)
            .expect("under the bound")
            .objects
            .len(),
        names.len()
    );
    // A turn past the bound on its own: answered alone, refused beside another.
    store.apply(&Write::Said {
        conversation: 0,
        words: routes::MOST_CONTENT_BYTES + 10,
    });
    let objects = store.lines.lines.objects().expect("reads");
    let (large, _) = objects
        .iter()
        .find(|(_, object)| {
            prodrome_core::event::canonical_envelope(object).len() > routes::MOST_CONTENT_BYTES
        })
        .expect("the large line");
    let alone = suzatary_core::query::Hashes::parse(Some(large.as_str())).expect("a set");
    assert_eq!(
        routes::content(&store.lines.lines, &alone)
            .expect("one object is answered")
            .objects
            .len(),
        1
    );
    let mut two = [large.as_str(), names[0]];
    two.sort_unstable();
    let two = suzatary_core::query::Hashes::parse(Some(&two.join(","))).expect("a set");
    assert_eq!(
        routes::content(&store.lines.lines, &two)
            .expect_err("past the bound")
            .status(),
        suzatary_core::refusal::Status::BadRequest
    );
}
