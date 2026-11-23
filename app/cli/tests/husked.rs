//! The lines' store keeps a husk in place of a line no key opens (`EventStore::husk`): a prune
//! changes no reading, writes nothing the second time, and takes back no whole it let go.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use proptest::prelude::*;

use prodrome_core::event::{Actor, Envelope, Hash, canonical_envelope};
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::Entry;
use suzatary_io_chains::conversations::ConversationStores;
use suzatary_lines::secretary::conversations::Conversations;
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_lines::secretary::line_shape::split;
use suzatary_lines::secretary::prune;
use suzatary_lines::secretary::transcript;
use suzatary_memory::secretary::erasure::Erasure;
use suzatary_memory::secretary::line_words::LineWords;
use suzatary_memory::secretary::locked_text::{KeyName, TextKey};
use suzatary_memory::secretary::slots::Slots;
use suzatary_memory::secretary::words::{Gone, Words};
use suzatary_store::store::{Doc as _, Gets as _, Prodrome as _, Reads as _};

/// The stores at `root` as the box opens them, with one fold per keys directory.
fn stores_at(root: &Path) -> ConversationStores {
    ConversationStores::open(
        root.join("secretary-lines"),
        root.join("secretary-lines-keys"),
        root.join("secretary-lines-runs"),
        Arc::new(suzatary_base::capability::Nothing),
        Arc::new(suzatary_base::capability::Nothing),
    )
}

fn at(minute: u32) -> prodrome_core::literal::Datetime {
    prodrome_core::literal::Datetime::new(2026, 10, 9, 9, minute, 0, 0).expect("an instant")
}

fn said(k: u32) -> Entry {
    Entry::Said {
        at: at(k),
        turn: if k % 2 == 1 {
            Turn::Person {
                text: format!("line {k}: what was said"),
                references: Vec::new(),
                attachments: Vec::new(),
            }
        } else {
            Turn::Model {
                text: format!(
                    "= Answer {k}\n\n{}",
                    "A paragraph of the answer. ".repeat(20)
                ),
                calls: Vec::new(),
            }
        },
        by: Some("ada".into()),
    }
}

fn id(n: usize) -> ConversationId {
    ConversationId::new(format!("2026-10-09-{n}")).expect("a slug")
}

/// Every reading a prune must keep, at one instant.
fn readings(conversations: Conversations<'_>) -> [(&'static str, String); 3] {
    suzatary_api::routes::lines_readings(conversations, at(50)).expect("reads")
}

/// Three conversations of six lines, the last with three past the content answer's bound and the
/// first two's keys destroyed: answers the lines' names that no key opens.
fn written(root: &Path) -> BTreeSet<Hash> {
    let held = stores_at(root);
    let conversations = held.conversations();
    for n in 0..3 {
        let opened = Entry::Opened {
            at: at(0),
            title: suzatary_memory::secretary::words::Words::kept("Long".to_owned()),
            model: "scripted".to_owned(),
            kind: suzatary_conversations::secretary::store::Kind::Topic,
        };
        transcript::start(conversations, &id(n), &opened).expect("started");
        for k in 1..6 {
            transcript::push(conversations, &id(n), &said(k)).expect("said");
        }
    }
    // Lines past the bound one content answer carries, so the readings ask as a device does.
    for (k, bytes) in [(6, 300_000), (7, 300_000), (8, 1_100_000)] {
        let large = Entry::Said {
            at: at(k),
            turn: Turn::Model {
                text: format!("= Long {k}\n\n{}", "word ".repeat(bytes / 5)),
                calls: Vec::new(),
            },
            by: None,
        };
        transcript::push(conversations, &id(2), &large).expect("said");
    }
    for n in 0..2 {
        let answers: BTreeSet<_> = transcript::read(conversations, &id(n))
            .expect("reads")
            .lines()
            .iter()
            .skip(2)
            .step_by(2)
            .flat_map(|written| written.line.keys().cloned())
            .collect();
        let by = Actor::new("ada").expect("an actor");
        transcript::erase(
            conversations,
            &id(n),
            answers,
            Erasure::Asked { by },
            at(40),
        )
        .expect("erased");
    }
    prune::census(conversations, usize::MAX)
        .expect("a census")
        .dead
}

fn file(root: &Path, name: &Hash) -> Vec<u8> {
    std::fs::read(
        root.join("secretary-lines/objects")
            .join(format!("{}.py", name.as_str())),
    )
    .expect("the file")
}

/// THE PRUNE LAW: a store pruned in any number of steps (a stop between two writes is a prune
/// of fewer names) reads as it did, opened afresh too, each husk holds no sealed word, and verify
/// finds nothing.
#[test]
fn a_pruned_store_reads_as_it_did_and_a_second_prune_writes_nothing() {
    let dir = suzatary_testkit::scratch("husked");
    let dead = written(dir.path());
    assert!(dead.len() >= 4, "{dead:?}");
    let held = stores_at(dir.path());
    let before = readings(held.conversations());
    let first: BTreeSet<Hash> = dead.iter().take(dead.len() / 2).cloned().collect();
    for names in [&first, &dead] {
        held.lines.husk(names).expect("pruned");
        assert_eq!(
            readings(held.conversations()),
            before,
            "after {} of them",
            names.len()
        );
    }
    let fresh = stores_at(dir.path());
    assert_eq!(readings(fresh.conversations()), before, "a fresh read");
    for name in &dead {
        let file = String::from_utf8(file(dir.path(), name)).expect("text");
        assert!(
            !file.contains("Scrubbed(") && !file.contains("Locked("),
            "{file}"
        );
    }
    assert_eq!(fresh.lines.verify(), Vec::<String>::new());
    assert_eq!(fresh.lines.husk(&dead).expect("pruned").objects, 0);
    let none = prune::stray(fresh.conversations()).expect("reads");
    assert!(none.is_empty(), "{none:?}");

    // A husk the prune did not write, under a line a key still opens, is named.
    let live = fresh
        .lines
        .objects()
        .expect("reads")
        .into_iter()
        .find_map(|(name, object)| match object {
            prodrome_core::event::Envelope::Change(change)
                if prodrome_core::store::Husking::holds_sealed(&change.event) =>
            {
                Some((name, prodrome_core::store::HuskObject::of(&change)?))
            }
            _ => None,
        })
        .expect("a live line");
    std::fs::write(
        dir.path()
            .join("secretary-lines/objects")
            .join(format!("{}.py", live.0.as_str())),
        live.1.print(),
    )
    .expect("written");
    let put = stores_at(dir.path());
    let named = prune::stray(put.conversations()).expect("reads");
    assert_eq!(named.len(), 1, "{named:?}");
}

/// A key store read as itself, each slot asked once, counting the calls to `open`: a slot's
/// binding is printed inside `open` alone, so the count is the bindings printed.
struct Counted<S> {
    slots: S,
    read: RefCell<BTreeMap<KeyName, Words<LineWords>>>,
    bound: Cell<usize>,
}

impl<S: Slots> Slots for Counted<S> {
    fn words(
        &self,
        name: &KeyName,
        open: &dyn Fn(&TextKey) -> Option<Words<LineWords>>,
    ) -> Words<LineWords> {
        if let Some(words) = self.read.borrow().get(name) {
            return words.clone();
        }
        let words = self.slots.words(name, &|key| {
            self.bound.set(self.bound.get() + 1);
            open(key)
        });
        self.read.borrow_mut().insert(name.clone(), words.clone());
        words
    }
}

/// Each conversation's lines opened through a fresh [`Counted`] reader of `conversations`' keys:
/// the lines' entries, the bindings printed, and the slots that open.
fn counted(
    conversations: Conversations<'_>,
    objects: &[(Hash, Envelope<LineEvent>)],
) -> (BTreeMap<Hash, Entry>, usize, usize) {
    let reads = conversations.reads();
    let slots = Counted {
        slots: |name: &KeyName| reads.key(name),
        read: RefCell::default(),
        bound: Cell::new(0),
    };
    let entries = transcript::opened(objects, &slots);
    let opened = slots
        .read
        .borrow()
        .values()
        .filter(|words| {
            !matches!(
                words,
                Words::Gone {
                    gone: Gone::Missing
                }
            )
        })
        .count();
    (entries, slots.bound.get(), opened)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, ..ProptestConfig::default() })]

    /// THE HUSKED CHAIN LAW: a store whose slots are sealed again 1 to 5 times, then pruned so
    /// every seal of a chain but the last is a husk, reads as it did whole and prints a binding
    /// for exactly the slots that open, one each.
    #[test]
    fn a_husked_reseal_chain_reads_as_it_did_and_binds_only_what_opens(
        chains in proptest::collection::vec((0usize..3, 1usize..=5), 1..8),
    ) {
        husked_chains(&chains)?;
    }
}

/// The husked chain law over two chains in one conversation, the first four seals deep.
#[test]
fn a_second_chain_after_a_deep_one_in_the_same_conversation() {
    husked_chains(&[(0, 1), (2, 4), (2, 4), (1, 2), (2, 5)]).expect("the law holds");
}

/// The husked chain law over `chains`, each a conversation and how many times its slots are
/// sealed again.
fn husked_chains(chains: &[(usize, usize)]) -> Result<(), TestCaseError> {
    let dir = suzatary_testkit::scratch("husked");
    let before = chains_written(dir.path(), chains);
    let held = stores_at(dir.path());
    let conversations = held.conversations();
    transcript::sweep(conversations).expect("swept");
    let dead = prune::census(conversations, usize::MAX)
        .expect("a census")
        .dead;
    let seals: usize = chains.iter().map(|(_, depth)| depth).sum();
    prop_assert_eq!(
        dead.len(),
        seals,
        "every seal of a chain but the last is husked"
    );
    held.lines.husk(&dead).expect("pruned");

    let fresh = stores_at(dir.path());
    let conversations = fresh.conversations();
    let objects = fresh.lines.objects().expect("objects");
    let (entries, bound, opened) = counted(conversations, &objects);
    prop_assert_eq!(readings(conversations), before.0);
    prop_assert_eq!(entries, before.1);
    let kept: BTreeSet<KeyName> = objects
        .iter()
        .filter_map(|(_, object)| object.event())
        .flat_map(LineEvent::keys)
        .filter(|name| matches!(fresh.keys.get(name), Ok(Some(_))))
        .cloned()
        .collect();
    prop_assert_eq!(opened, kept.len(), "every slot whose key is held opens");
    prop_assert_eq!(
        bound,
        opened,
        "a binding for each slot that opens, and none for a husk"
    );
    Ok(())
}

/// A store at `root` holding `chains`, each a person's line in its conversation with every slot
/// sealed again as many times, each seal from the one before: its readings and its lines' entries.
fn chains_written(
    root: &Path,
    chains: &[(usize, usize)],
) -> ([(&'static str, String); 3], BTreeMap<Hash, Entry>) {
    let held = stores_at(root);
    let conversations = held.conversations();
    let second = |n: usize| {
        prodrome_core::literal::Datetime::new(
            2026,
            10,
            9,
            9,
            20 + u32::try_from(n / 60).expect("a minute"),
            u32::try_from(n % 60).expect("a second"),
            0,
        )
        .expect("an instant")
    };
    let mut written = 0;
    let mut started = BTreeSet::new();
    for (conversation, depth) in chains {
        let id = id(*conversation);
        written += 1;
        if started.insert(*conversation) {
            let opened = Entry::Opened {
                at: second(written),
                title: Words::kept("Chains".to_owned()),
                model: "scripted".to_owned(),
                kind: suzatary_conversations::secretary::store::Kind::Topic,
            };
            transcript::start(conversations, &id, &opened).expect("started");
        }
        written += 1;
        let entry = Entry::Said {
            at: second(written),
            turn: Turn::Person {
                text: format!("question {written}"),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: Some("ada".into()),
        };
        transcript::push(conversations, &id, &entry).expect("said");
        let read = transcript::read(conversations, &id).expect("reads");
        let mut count = u32::try_from(read.lines().len()).expect("a count");
        let first: Vec<KeyName> = read
            .lines()
            .last()
            .expect("the line")
            .line
            .keys()
            .cloned()
            .collect();
        let words = split(&entry).1.expect("words");
        for (of, words) in first.into_iter().zip(&words) {
            let mut from = of.clone();
            for _ in 0..*depth {
                written += 1;
                let (line, key) = LineEvent::reseal(
                    id.clone(),
                    second(written),
                    transcript::writer(),
                    count,
                    of.clone(),
                    from,
                    &suzatary_held::redact::Scrubbed::of(words).expect("words through the door"),
                    &suzatary_io_local::texts::SystemEntropy,
                )
                .expect("sealed again");
                held.keys.set(&key.name(), &key).expect("kept");
                held.lines.append(line).expect("appended");
                from = key.name();
                count += 1;
            }
        }
    }
    let whole = held.lines.objects().expect("objects");
    (readings(conversations), counted(conversations, &whole).0)
}

/// Offered a device's whole dead lines and a line written over them, a pruned store takes the new
/// line alone, keeps each husk, and still writes over them.
#[test]
fn a_pruned_store_takes_back_no_dropped_whole() {
    let dir = suzatary_testkit::scratch("husked");
    let (at_box, at_device) = (dir.path().join("box"), dir.path().join("device"));
    let dead = written(&at_box);
    copy(&at_box, &at_device);
    let device = stores_at(&at_device);
    let held = stores_at(&at_box);
    held.lines.husk(&dead).expect("pruned");
    let husks: Vec<_> = dead.iter().map(|name| file(&at_box, name)).collect();

    transcript::push(device.conversations(), &id(1), &said(7)).expect("said");
    let offered: Vec<(Hash, String)> = device
        .lines
        .objects()
        .expect("reads")
        .iter()
        .map(|(name, object)| (name.clone(), canonical_envelope(object)))
        .collect();
    let taken = held.lines.accept(&offered).expect("taken");
    assert_eq!(taken.len(), 1, "the new line alone: {taken:?}");
    let again: Vec<_> = dead.iter().map(|name| file(&at_box, name)).collect();
    assert_eq!(again, husks, "each husk is kept");

    transcript::push(held.conversations(), &id(1), &said(9)).expect("written over the husks");
    assert_eq!(held.lines.verify(), Vec::<String>::new());
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("made");
    for entry in std::fs::read_dir(from).expect("listed") {
        let entry = entry.expect("an entry");
        let path = to.join(entry.file_name());
        if entry.file_type().expect("a type").is_dir() {
            copy(&entry.path(), &path);
        } else {
            std::fs::copy(entry.path(), path).expect("copied");
        }
    }
}
