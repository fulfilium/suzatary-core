//! The memory's laws, over the stores held in core's memory.

use prodrome_core::event::Actor;
use prodrome_core::literal::Datetime;
use std::sync::atomic::{AtomicU8, Ordering};
use suzatary_base::capability::Entropy;
use suzatary_digest::secretary::digests::Digests;
use suzatary_inbox::secretary::inbox_event::InboxEvent;
use suzatary_memory::secretary::memory::{Fact, FactId, Kind, Origin};
use suzatary_memory::secretary::memory_event::{self, MemoryEvent};
use suzatary_memory::secretary::salted_text::{Text, TextName};
use suzatary_store::store::{Prodrome, Reads, WriteLock};

use suzatary_base::conversation_id::ConversationId;
use suzatary_digest::secretary::memories::*;
use suzatary_store::store::{Doc as _, InMemoryDoc};
use suzatary_stores::stores::InMemory;

/// Each salt one more than the last.
#[derive(Default)]
struct Counting(AtomicU8);

/// A digest over no lines, in memory.
#[derive(Default)]
struct Digested {
    lines: suzatary_stores::stores::conversations::InMemory,
    history: InMemory<suzatary_digest::secretary::digest_event::DigestEvent>,
    texts: InMemoryDoc<TextName, Text<suzatary_digest::secretary::digest_line::Line>>,
    owners: std::collections::BTreeSet<Actor>,
}

impl Digested {
    fn digests(&self) -> Digests<'_> {
        Digests {
            history: &self.history,
            texts: &self.texts,
            lines: self.lines.conversations(),
            owners: &self.owners,
            compacting: &suzatary_digest::secretary::compacting::Compacting::Off,
        }
    }
}

impl Entropy for Counting {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(self.0.fetch_add(1, Ordering::Relaxed));
        Ok(())
    }
}

fn at(minute: u32) -> Datetime {
    Datetime::new(2026, 10, 2, 9, minute, 0, 0).expect("an instant")
}

fn said() -> Origin {
    Origin::Turn {
        conversation: ConversationId::new("default").expect("a slug"),
        turn: 0,
    }
}

fn fact(text: &str) -> Fact {
    Fact::new(text).expect("a fact")
}

#[test]
fn forgetting_deletes_the_words_and_keeps_the_history() {
    let (history, texts, entropy) = (
        InMemory::default(),
        InMemoryDoc::default(),
        Counting::default(),
    );
    let cards = InMemory::<InboxEvent>::default();
    let digests = Digested::default();
    let memories = Memories {
        history: &history,
        texts: &texts,
        cards: &cards,
        entropy: &entropy,
        digests: digests.digests(),
    };
    let kept = memories
        .remember(
            &fact("He takes the 7:40 train"),
            Kind::Owner,
            &said(),
            at(0),
        )
        .expect("remembers");
    let gone = memories
        .remember(
            &fact("Mira is the accountant"),
            Kind::Person,
            &said(),
            at(1),
        )
        .expect("remembers");
    assert_eq!(texts.keys().len(), 2);
    let ada = Actor::new("ada").expect("an actor");
    let forgotten = memories.forget(gone.id, at(2), &ada).expect("forgets");
    assert_eq!((forgotten.id, forgotten.version), (FactId(2), 3));
    assert_eq!(texts.keys().len(), 1, "the forgotten words are deleted");
    let read = memories.reads().read().expect("reads");
    assert_eq!(read.facts, vec![kept]);
    assert_eq!(read.forgotten, vec![forgotten]);
    assert_eq!(read.version, 3);
    for (_, object) in history.objects().expect("objects") {
        let print = prodrome_core::event::canonical_envelope(&object);
        assert!(
            !print.contains("Mira") && !print.contains("7:40"),
            "{print}"
        );
    }
    assert!(memories.forget(gone.id, at(3), &ada).is_err());
    assert!(
        memories
            .remember(
                &fact("He takes the 7:40 train"),
                Kind::Owner,
                &said(),
                at(4)
            )
            .is_err(),
        "a duplicate"
    );
}

#[test]
fn a_lost_text_reads_as_absent_and_still_counts() {
    let (history, texts, entropy) = (
        InMemory::default(),
        InMemoryDoc::default(),
        Counting::default(),
    );
    let cards = InMemory::<InboxEvent>::default();
    let digests = Digested::default();
    let memories = Memories {
        history: &history,
        texts: &texts,
        cards: &cards,
        entropy: &entropy,
        digests: digests.digests(),
    };
    let lost = memories
        .remember(
            &fact("A fact whose file is lost"),
            Kind::Owner,
            &said(),
            at(0),
        )
        .expect("remembers");
    for name in texts.keys() {
        texts.remove(&name).expect("lost");
    }
    let read = memories.reads().read().expect("reads, never refuses");
    assert!(read.facts.is_empty());
    assert_eq!(read.version, 1);
    let history_read = memories.reads().history().expect("reads");
    assert_eq!(
        history_read.live().count(),
        1,
        "still standing in the history"
    );
    memories
        .forget(lost.id, at(1), &Actor::new("ada").expect("an actor"))
        .expect("a lost fact can still be forgotten");
}

/// A store that reads as its inner one until told to fail, and then refuses every read.
struct Failing<E: suzatary_stores::stores::Store> {
    inner: InMemory<E>,
    failing: std::sync::atomic::AtomicBool,
}

impl<E: suzatary_stores::stores::Store<Vocabulary: Default>> Failing<E> {
    fn new() -> Self {
        Self {
            inner: InMemory::default(),
            failing: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl<E: suzatary_stores::stores::Store> Reads<E> for Failing<E> {
    fn objects(
        &self,
    ) -> Result<
        Vec<(
            prodrome_core::event::Hash,
            prodrome_core::event::Envelope<E>,
        )>,
        suzatary_store::store::StoreError,
    > {
        if self.failing.load(Ordering::Relaxed) {
            return Err(suzatary_store::store::StoreError::unreadable(
                "failing", "told to",
            ));
        }
        self.inner.objects()
    }
    fn excluded(
        &self,
    ) -> Result<
        std::collections::BTreeSet<prodrome_core::event::Hash>,
        suzatary_store::store::StoreError,
    > {
        if self.failing.load(Ordering::Relaxed) {
            return Err(suzatary_store::store::StoreError::unreadable(
                "failing", "told to",
            ));
        }
        self.inner.excluded()
    }
    fn tips(&self) -> Result<Vec<prodrome_core::event::Hash>, suzatary_store::store::StoreError> {
        self.inner.tips()
    }
    fn verify(&self) -> Vec<String> {
        self.inner.verify()
    }
}

impl<E: suzatary_stores::stores::Store> Prodrome<E> for Failing<E> {
    fn append(
        &self,
        event: E,
    ) -> Result<prodrome_core::event::Hash, suzatary_store::store::StoreError> {
        self.inner.append(event)
    }
    fn accept(
        &self,
        objects: &[(prodrome_core::event::Hash, String)],
    ) -> Result<Vec<prodrome_core::event::Hash>, suzatary_store::store::StoreError> {
        self.inner.accept(objects)
    }
    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, suzatary_store::store::StoreError> {
        self.inner.locked()
    }
}

/// The text store is box-local and never pushed, so a text deleted wrongly is gone for good:
/// a sweep that cannot read every history that names texts deletes nothing.
#[test]
fn a_sweep_that_cannot_read_a_naming_deletes_nothing() {
    let (history, cards) = (Failing::<MemoryEvent>::new(), Failing::<InboxEvent>::new());
    let (texts, entropy) = (InMemoryDoc::default(), Counting::default());
    let (summaries, lines) = (
        Failing::<suzatary_digest::secretary::digest_event::DigestEvent>::new(),
        Failing::<suzatary_lines::secretary::line_event::LineEvent>::new(),
    );
    let digest_texts = InMemoryDoc::default();
    let memories = Memories {
        history: &history,
        texts: &texts,
        cards: &cards,
        entropy: &entropy,
        digests: Digests {
            history: &summaries,
            texts: &digest_texts,
            lines: suzatary_lines::secretary::conversations::Conversations {
                runs: &*suzatary_lines::nothing::RUNS,
                lines: &lines,
                keys: &suzatary_memory::secretary::locked_text::InMemoryKeys::default(),
                entropy: &entropy,
                renders: &suzatary_base::capability::Nothing,
                sessions: &suzatary_base::capability::Nothing,
                opened: &suzatary_lines::secretary::opened::Opened::default(),
            },
            owners: &std::collections::BTreeSet::new(),
            compacting: &suzatary_digest::secretary::compacting::Compacting::Off,
        },
    };
    let gone = memories
        .remember(&fact("A fact to forget"), Kind::Owner, &said(), at(0))
        .expect("remembers");
    let ada = Actor::new("ada").expect("an actor");
    let event = memory_event::forgotten(gone.id, at(1), ada, 2).expect("an event");
    history
        .inner
        .append(event)
        .expect("forgotten, not yet swept");
    assert_eq!(texts.keys().len(), 1);
    for failing in [
        &history.failing,
        &cards.failing,
        &summaries.failing,
        &lines.failing,
    ] {
        failing.store(true, Ordering::Relaxed);
        assert!(
            memories.sweep().is_err(),
            "the sweep says it could not read"
        );
        for half in [Texts::Facts, Texts::Summaries] {
            assert!(memories.sweep_only(half).is_err(), "and so does each half");
        }
        assert_eq!(texts.keys().len(), 1, "and deletes nothing");
        failing.store(false, Ordering::Relaxed);
    }
    assert_eq!(memories.sweep().expect("sweeps"), 1);
    assert!(texts.keys().is_empty(), "read again, the dead text goes");
    assert_eq!(
        memories.sweep().expect("sweeps"),
        0,
        "and the same sweep again says 0"
    );
    for half in [Texts::Facts, Texts::Summaries] {
        assert_eq!(memories.sweep_only(half).expect("sweeps"), 0);
    }
}
