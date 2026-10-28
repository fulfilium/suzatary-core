//! A pull of the lines that lacks nothing, at the size of the box's own store: about 14,700
//! objects over a hundred conversations, 7,000 of them reseals. The replica names the frontier
//! its last answer named, and the answer carries no node and no key, fits in a kilobyte and reads
//! no object of the store: the law is the count of the store's reads, never a clock, so a gate
//! under load cannot fail it. The time is printed as a measurement. A line written after it is
//! sent alone, with its own key, from one read of the store.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use prodrome_core::event::{Actor, Envelope, Hash, canonical_envelope};
use prodrome_core::literal::Datetime;
use prodrome_core::store::{MemoryStore, Replica as _};

use prodrome_core::store::frontier::{Frontiers, KEPT, Since, Token};
use suzatary_core::capability::Turn;
use suzatary_core::routes::{self, Books, Cut};
use suzatary_core::secretary::conversations::{Conversations, InMemory};
use suzatary_core::secretary::line_event::LineEvent;
use suzatary_core::secretary::line_words::LineWords;
use suzatary_core::secretary::locked_text::TextKey;
use suzatary_core::secretary::store::{ConversationId, Entry, Kind};
use suzatary_core::secretary::words::Words;
use suzatary_core::store::{Doc as _, Prodrome, Reads, StoreError, WriteLock};

const CONVERSATIONS: usize = 100;
const LINES: usize = 7_000;

fn at(n: usize) -> Datetime {
    let n = u32::try_from(n).expect("a count");
    Datetime::new(2026, 9, 30, n / 3600, (n / 60) % 60, n % 60, 0).expect("an instant")
}

fn id(n: usize) -> ConversationId {
    ConversationId::new(format!("2026-09-30-{n}")).expect("a slug")
}

/// The lines as the box holds them after a migration: each conversation opened, `LINES` turns
/// spread over them, and each turn sealed again from its own seal, so about half the store is
/// reseals. The first line begins the store; the rest are appended through one store in memory
/// holding it and handed over at once, since an append each reads the store again.
fn store() -> InMemory {
    let lines = InMemory::default();
    let writer = Actor::new("assistant").expect("an actor");
    let mut held: BTreeMap<usize, u32> = BTreeMap::new();
    let mut events: Vec<LineEvent> = Vec::new();
    let mut write = |conversation: usize, line: LineEvent, keys: &[TextKey]| {
        for key in keys {
            lines.keys.set(&key.name(), key).expect("kept");
        }
        events.push(line);
        *held.entry(conversation).or_default() += 1;
    };
    for n in 0..CONVERSATIONS {
        let opened = Entry::Opened {
            at: at(n),
            title: Words::kept(format!("Topic {n}")),
            model: "a-cli/small".to_owned(),
            kind: Kind::Topic,
        };
        let (line, keys) =
            LineEvent::write(id(n), writer.clone(), 0, &opened, &lines.entropy).expect("a line");
        write(n, line, &keys);
    }
    let mut count: BTreeMap<usize, u32> = (0..CONVERSATIONS).map(|n| (n, 1)).collect();
    for n in 0..LINES {
        let conversation = n % CONVERSATIONS;
        let text = format!("Question {n} on the permit, asked again.");
        let said = Entry::Said {
            at: at(CONVERSATIONS + 2 * n),
            turn: Turn::Person {
                text: text.clone(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: None,
        };
        let held = count.entry(conversation).or_default();
        let (line, keys) = LineEvent::write(
            id(conversation),
            writer.clone(),
            *held,
            &said,
            &lines.entropy,
        )
        .expect("a line");
        let of = keys[0].name();
        write(conversation, line, &keys);
        let (reseal, key) = LineEvent::reseal(
            id(conversation),
            at(CONVERSATIONS + 2 * n + 1),
            writer.clone(),
            *held + 1,
            of.clone(),
            of,
            &LineWords::Person {
                text,
                attachments: Vec::new(),
            },
            &lines.entropy,
        )
        .expect("a reseal");
        write(conversation, reseal, &[key]);
        *held += 2;
    }
    let mut events = events.into_iter();
    lines
        .lines
        .append(events.next().expect("a first line"))
        .expect("begun");
    let begun: BTreeMap<Hash, Vec<u8>> = lines
        .lines
        .objects()
        .expect("reads")
        .iter()
        .map(|(name, object)| (name.clone(), canonical_envelope(object).into_bytes()))
        .collect();
    let memory: MemoryStore<LineEvent> = MemoryStore::default();
    memory
        .receive(begun.keys().cloned().collect(), &|name| {
            begun.get(name).cloned()
        })
        .expect("received");
    for event in events {
        memory.append(event).expect("appended");
    }
    let prints: Vec<(Hash, String)> = memory
        .held()
        .expect("held")
        .dag
        .objects()
        .iter()
        .filter(|(name, _)| !begun.contains_key(*name))
        .map(|(name, object)| (name.clone(), canonical_envelope(object)))
        .collect();
    lines.lines.accept(&prints).expect("taken");
    lines
}

/// The lines, counting every read of the whole store and every read of objects by name.
struct Counted<'a> {
    lines: &'a dyn Prodrome<LineEvent>,
    reads: AtomicUsize,
    graph: std::sync::Mutex<Option<std::sync::Arc<suzatary_core::store::Graph<LineEvent>>>>,
}

impl Counted<'_> {
    fn reads(&self) -> usize {
        self.reads.swap(0, Ordering::SeqCst)
    }
}

impl Reads<LineEvent> for Counted<'_> {
    /// A graph the store built again, rather than the one it kept, is a read of the whole store.
    fn graph(&self) -> Result<std::sync::Arc<suzatary_core::store::Graph<LineEvent>>, StoreError> {
        let graph = self.lines.graph()?;
        let mut last = self
            .graph
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !last
            .as_ref()
            .is_some_and(|kept| std::sync::Arc::ptr_eq(kept, &graph))
        {
            self.reads.fetch_add(1, Ordering::SeqCst);
        }
        *last = Some(std::sync::Arc::clone(&graph));
        Ok(graph)
    }
    fn objects(&self) -> Result<Vec<(Hash, Envelope<LineEvent>)>, StoreError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.lines.objects()
    }
    fn literals(&self, names: &[Hash]) -> Result<Vec<Option<String>>, StoreError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.lines.literals(names)
    }
    fn sizes(&self, names: &[Hash]) -> Result<Vec<Option<usize>>, StoreError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.lines.sizes(names)
    }
    fn excluded(&self) -> Result<std::collections::BTreeSet<Hash>, StoreError> {
        self.lines.excluded()
    }
    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        self.lines.tips()
    }
    fn verify(&self) -> Vec<String> {
        self.lines.verify()
    }
}

impl Prodrome<LineEvent> for Counted<'_> {
    fn append(&self, event: LineEvent) -> Result<Hash, StoreError> {
        self.lines.append(event)
    }
    fn accept(&self, objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError> {
        self.lines.accept(objects)
    }
    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        self.lines.locked()
    }
}

#[test]
fn a_pull_that_lacks_nothing_sends_nothing_and_reads_no_object() {
    let lines = store();
    let counted = Counted {
        lines: &lines.lines,
        reads: AtomicUsize::new(0),
        graph: std::sync::Mutex::default(),
    };
    let conversations = Conversations {
        lines: &counted,
        ..lines.conversations()
    };
    let (epochs, frontiers) = (
        suzatary_core::epoch::Epochs::remembered(),
        Frontiers::remembered(KEPT),
    );
    let books = Books {
        epochs: &epochs,
        frontiers: &frontiers,
    };
    let now = at(3 * LINES);
    let whole = routes::lines_skeleton(conversations, &Since::Nothing, books, now, Cut::Whole)
        .expect("the skeleton");
    assert!(whole.nodes.len() > 2 * LINES, "{} nodes", whole.nodes.len());
    let since = Since::Frontier(Token::parse(&whole.frontier.digest).expect("a token"));
    counted.reads();

    let started = Instant::now();
    let known =
        routes::lines_skeleton(conversations, &since, books, now, Cut::PIECE).expect("the pull");
    let took = started.elapsed();
    let bytes = serde_json::to_vec(&known).expect("prints").len();
    assert_eq!(known.since, suzatary_core::wire::SinceRead::Known);
    assert!(known.nodes.is_empty() && known.keys.is_empty() && known.rest == 0);
    assert_eq!(
        known.frontier, whole.frontier,
        "the replica stands where it stood"
    );
    assert!(bytes < 1024, "{bytes} bytes");
    assert_eq!(counted.reads(), 0, "no object of the store was read");
    println!("a pull that lacks nothing: {bytes} bytes in {took:?}");

    let said = Entry::Said {
        at: now,
        turn: Turn::Person {
            text: "One more.".to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: None,
    };
    let count = u32::try_from(1 + 2 * (LINES / CONVERSATIONS)).expect("a count");
    let (line, keys) = LineEvent::write(
        id(0),
        Actor::new("assistant").expect("an actor"),
        count,
        &said,
        &lines.entropy,
    )
    .expect("a line");
    for key in &keys {
        lines.keys.set(&key.name(), key).expect("kept");
    }
    lines.lines.append(line).expect("appended");
    let next =
        routes::lines_skeleton(conversations, &since, books, now, Cut::PIECE).expect("the pull");
    assert_eq!(next.nodes.len(), 1);
    assert_eq!(
        next.keys.keys().cloned().collect::<Vec<_>>(),
        keys.iter()
            .map(|key| key.name().as_str().to_owned())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        counted.reads(),
        2,
        "the store read once, and the new node sized"
    );
}
