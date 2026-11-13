//! The lines' store keeps a husk in place of a line no key opens (`suzatary_io::husked`): a prune
//! changes no reading, writes nothing the second time, and takes back no whole it let go.

use std::collections::BTreeSet;
use std::path::Path;

use prodrome_core::event::{Actor, Hash, canonical_envelope};
use suzatary_base::conversation_id::ConversationId;
use suzatary_core::secretary::conversations::Conversations;
use suzatary_core::secretary::erasure::Erasure;
use suzatary_core::secretary::model::Turn;
use suzatary_core::secretary::store::Entry;
use suzatary_core::secretary::transcript;
use suzatary_core::store::{Prodrome as _, Reads as _};
use suzatary_io::husked::Husked;
use suzatary_io::objects::Objects;
use suzatary_io::text_keys::TextKeys;

type Lines = Objects<
    suzatary_core::secretary::line_event::LineEvent,
    Husked<suzatary_core::secretary::line_event::LineEvent>,
>;

fn lines_at(root: &Path) -> (Lines, TextKeys) {
    (
        Objects::husked(root.join("secretary-lines")),
        suzatary_io::text_keys::open(root.join("secretary-lines-keys")),
    )
}

fn stores<'s>(lines: &'s Lines, keys: &'s TextKeys) -> Conversations<'s> {
    static OPENED: std::sync::LazyLock<suzatary_core::secretary::opened::Opened> =
        std::sync::LazyLock::new(Default::default);
    Conversations {
        lines,
        keys,
        entropy: &suzatary_io::texts::SystemEntropy,
        renders: &suzatary_base::capability::Nothing,
        sessions: &suzatary_base::capability::Nothing,
        opened: &OPENED,
    }
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

/// Three conversations of six lines, the last with three lines past the content answer's bound,
/// and the first two's answers erased and their keys destroyed: answers the lines' names that no
/// key opens.
fn written(root: &Path) -> BTreeSet<Hash> {
    let (lines, keys) = lines_at(root);
    let conversations = stores(&lines, &keys);
    for n in 0..3 {
        let opened = Entry::Opened {
            at: at(0),
            title: suzatary_core::secretary::words::Words::kept("Long".to_owned()),
            model: "scripted".to_owned(),
            kind: suzatary_core::secretary::store::Kind::Topic,
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
    suzatary_core::secretary::prune::census(conversations, usize::MAX)
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
/// of fewer names) reads as it did, and so do the same files opened afresh, as a clone of them
/// reads; each husk holds no sealed word; verify finds nothing; and a second prune writes nothing.
#[test]
fn a_pruned_store_reads_as_it_did_and_a_second_prune_writes_nothing() {
    let dir = tempfile::tempdir().expect("a scratch dir");
    let dead = written(dir.path());
    assert!(dead.len() >= 4, "{dead:?}");
    let (lines, keys) = lines_at(dir.path());
    let before = readings(stores(&lines, &keys));
    let first: BTreeSet<Hash> = dead.iter().take(dead.len() / 2).cloned().collect();
    for names in [&first, &dead] {
        lines.husk(names).expect("pruned");
        assert_eq!(
            readings(stores(&lines, &keys)),
            before,
            "after {} of them",
            names.len()
        );
    }
    let (fresh, keys) = lines_at(dir.path());
    assert_eq!(readings(stores(&fresh, &keys)), before, "a fresh read");
    for name in &dead {
        let file = String::from_utf8(file(dir.path(), name)).expect("text");
        assert!(
            !file.contains("Scrubbed(") && !file.contains("Locked("),
            "{file}"
        );
    }
    assert_eq!(fresh.verify(), Vec::<String>::new());
    assert_eq!(fresh.husk(&dead).expect("pruned").objects, 0);
    let none = suzatary_core::secretary::prune::stray(stores(&fresh, &keys)).expect("reads");
    assert!(none.is_empty(), "{none:?}");

    // A husk the prune did not write, under a line a key still opens, is named.
    let live = fresh
        .objects()
        .expect("reads")
        .into_iter()
        .find_map(|(name, object)| match object {
            prodrome_core::event::Envelope::Change(change)
                if suzatary_core::store::Husking::holds_sealed(&change.event) =>
            {
                Some((name, suzatary_core::store::HuskObject::of(&change)?))
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
    let (put, keys) = lines_at(dir.path());
    let named = suzatary_core::secretary::prune::stray(stores(&put, &keys)).expect("reads");
    assert_eq!(named.len(), 1, "{named:?}");
}

/// A pruned store takes back no whole it let go: offered every object of a device that still
/// holds the dead lines whole, and a line that device wrote over them, it takes the new line
/// alone and keeps each husk. It still writes over its husks.
#[test]
fn a_pruned_store_takes_back_no_dropped_whole() {
    let dir = tempfile::tempdir().expect("a scratch dir");
    let (at_box, at_device) = (dir.path().join("box"), dir.path().join("device"));
    let dead = written(&at_box);
    copy(&at_box, &at_device);
    let (device, device_keys) = lines_at(&at_device);
    let (lines, keys) = lines_at(&at_box);
    lines.husk(&dead).expect("pruned");
    let husks: Vec<_> = dead.iter().map(|name| file(&at_box, name)).collect();

    transcript::push(stores(&device, &device_keys), &id(1), &said(7)).expect("said");
    let offered: Vec<(Hash, String)> = device
        .objects()
        .expect("reads")
        .iter()
        .map(|(name, object)| (name.clone(), canonical_envelope(object)))
        .collect();
    let taken = lines.accept(&offered).expect("taken");
    assert_eq!(taken.len(), 1, "the new line alone: {taken:?}");
    let again: Vec<_> = dead.iter().map(|name| file(&at_box, name)).collect();
    assert_eq!(again, husks, "each husk is kept");

    transcript::push(stores(&lines, &keys), &id(1), &said(9)).expect("written over the husks");
    assert_eq!(lines.verify(), Vec::<String>::new());
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
