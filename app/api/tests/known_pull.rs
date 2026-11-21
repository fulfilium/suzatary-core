//! A pull of the lines that lacks nothing, at the size of the box's own store: about 14,700
//! objects over a hundred conversations, 7,000 of them reseals. The replica names the frontier
//! its last answer named, and the answer carries no node and no key, fits in a kilobyte and reads
//! no object of the store: the law is the count of the store's reads, never a clock, so a gate
//! under load cannot fail it. The time is printed as a measurement. A line written after it is
//! sent alone, with its own key, from one read of the store.

use std::collections::BTreeMap;
use std::time::Instant;

use prodrome_core::event::{Actor, Hash, canonical_envelope};
use prodrome_core::literal::Datetime;
use prodrome_core::store::{MemoryStore, Replica as _};

use prodrome_core::store::frontier::{Frontiers, KEPT, Since, Token};
use suzatary_api::routes::{self, Books, Cut};
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::{Entry, Kind};
use suzatary_lines::secretary::conversations::Conversations;
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_memory::secretary::line_words::LineWords;
use suzatary_memory::secretary::locked_text::TextKey;
use suzatary_memory::secretary::words::Words;
use suzatary_store::store::{Counted, Doc as _, Prodrome as _, Reads as _};
use suzatary_stores::stores::conversations::InMemory;

const CONVERSATIONS: usize = 100;
const LINES: usize = 7_000;

fn at(n: usize) -> Datetime {
    let n = u32::try_from(n).expect("a count");
    Datetime::new(2026, 9, 30, n / 3600, (n / 60) % 60, n % 60, 0).expect("an instant")
}

pub(crate) fn id(n: usize) -> ConversationId {
    ConversationId::new(format!("2026-09-30-{n}")).expect("a slug")
}

/// The lines as the box holds them after a migration: each conversation opened, `LINES` turns
/// spread over them, and each turn sealed again from its own seal, so about half the store is
/// reseals. The first line begins the store; the rest are appended through one store in memory
/// holding it and handed over at once, since an append each reads the store again.
pub(crate) fn store() -> InMemory {
    shaped(0)
}

/// [`store`], its first `long` turns all in conversation 0: with 2,500, that conversation holds
/// 5,001 lines of the store's 14,100.
pub(crate) fn shaped(long: usize) -> InMemory {
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
        let conversation = if n < long { 0 } else { n % CONVERSATIONS };
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
            &suzatary_held::redact::Scrubbed::of(&LineWords::Person {
                text,
                attachments: Vec::new(),
            })
            .expect("words through the door"),
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
    // The box's own lines, copied: their store is the copy's keeper of record.
    memory
        .receive(
            begun.keys().cloned().collect(),
            &|name| begun.get(name).cloned(),
            prodrome_core::store::Sender::Keeper,
        )
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

#[test]
fn a_pull_that_lacks_nothing_sends_nothing_and_reads_no_object() {
    let lines = store();
    let counted = Counted::new(&lines.lines);
    let conversations = Conversations {
        lines: &counted,
        ..lines.conversations()
    };
    let (epochs, frontiers) = (
        suzatary_signatures::epoch::Epochs::remembered(),
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
    assert_eq!(known.since, suzatary_store::wire::store::SinceRead::Known);
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
