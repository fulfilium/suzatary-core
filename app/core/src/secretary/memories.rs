//! The memory's two stores as one value: its history, a prodrome at the
//! [`MemoryEvent`] schema that names each fact's words, and its texts, the words themselves,
//! which forgetting deletes. A remember card in the inbox names its words in the same text store,
//! so the inbox is read here too.
//!
//! Every reading is a function of the history and takes no lock; a fact whose text is missing,
//! forgotten or lost, reads as absent, never as an error. The questions that are not monotone,
//! and so are asked under the history's hold ([`Held`]), are the writes' own: is there room under
//! [`MAX_LIVE`], does a standing fact already say this, does this id still stand, what is the next
//! number, and which card already keeps these words. Whether the secretary may remember at all is
//! `secretary::taint`'s, asked of a conversation's turns that the call already rests on.

use prodrome_core::event::Actor;
use prodrome_core::literal::Datetime;

use crate::capability::Entropy;
use crate::refusal::Refusal;
use crate::secretary::digests::Digests;
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::memory::{
    Fact, FactId, Forgotten, Kind, MAX_LIVE, Memory, Origin, Remembered,
};
use crate::secretary::memory_event::{self, MemoryEvent};
use crate::secretary::salted_text::{Text, TextName};
use crate::secretary::store::Proposal;
use crate::secretary::two_phase::TwoPhase;
use crate::store::{Doc, Prodrome, WriteLock};

/// The memory: its history, its texts, the inbox whose remember cards name texts too, and the
/// salt each new text is kept with.
#[derive(Clone, Copy)]
pub struct Memories<'a> {
    /// What was remembered and forgotten, by id; never a fact's words.
    pub history: &'a dyn Prodrome<MemoryEvent>,
    /// The words of each standing fact and of each remember card, by the name the history or the
    /// card gives them.
    pub texts: &'a dyn Doc<TextName, Text<Fact>>,
    /// The inbox, whose remember cards name their words in [`Memories::texts`].
    pub cards: &'a dyn Prodrome<InboxEvent>,
    /// Where a new text's salt comes from.
    pub entropy: &'a dyn Entropy,
    /// The digest, whose summaries' words the same sweep deletes.
    pub digests: Digests<'a>,
}

/// The memory under its hold: a write, and a deletion of words, is made only through one.
pub struct Held<'a> {
    memories: Memories<'a>,
    _hold: Box<dyn WriteLock + 'a>,
}

/// The words a remember card names: found on a card already, or kept for this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kept {
    /// A card already names these words; asking again names them too.
    Found(TextName),
    /// Kept now, for a card not yet filed.
    New(TextName),
}

impl Kept {
    /// The name either way.
    #[must_use]
    pub fn name(&self) -> &TextName {
        match self {
            Kept::Found(name) | Kept::New(name) => name,
        }
    }
}

impl<'a> Memories<'a> {
    /// The history's reading.
    ///
    /// # Errors
    /// The store's own.
    pub fn history(&self) -> Result<TwoPhase, Refusal> {
        Ok(TwoPhase::of(&self.history.nodes()?))
    }

    /// The memory as a reader gets it: every standing fact whose words read.
    ///
    /// # Errors
    /// The history's store's own, and the texts': a text that will not read is a failure, never
    /// an absent fact.
    pub fn read(&self) -> Result<Memory, Refusal> {
        self.with_words(&self.history()?)
    }

    /// The words kept under `name`, for a card or a fact; `None` once they are deleted.
    ///
    /// # Errors
    /// The text store's own, for words that are there and will not read.
    pub fn words(&self, name: &TextName) -> Result<Option<Fact>, Refusal> {
        Ok(self.texts.get(name)?.map(|text| text.words().clone()))
    }

    /// `history` with each standing fact's words.
    fn with_words(&self, history: &TwoPhase) -> Result<Memory, Refusal> {
        let mut kept = std::collections::BTreeMap::new();
        for (_, standing) in history.live() {
            if let Some(fact) = self.words(&standing.text)? {
                kept.insert(standing.text.clone(), fact);
            }
        }
        Ok(Memory::from_history(history, |name| {
            kept.get(name).cloned()
        }))
    }

    /// Take the history's hold, which every write and every deletion of words is made under.
    ///
    /// # Errors
    /// The store's own.
    pub fn hold(&self) -> Result<Held<'a>, Refusal> {
        Ok(Held {
            memories: *self,
            _hold: self.history.locked()?,
        })
    }

    /// Remember a fact under the history's hold, as [`Held::remember`].
    ///
    /// # Errors
    /// As [`Held::remember`].
    pub fn remember(
        &self,
        fact: &Fact,
        kind: Kind,
        said: &Origin,
        at: Datetime,
    ) -> Result<Remembered, Refusal> {
        self.hold()?.remember(fact, kind, said, at)
    }

    /// Forget a standing fact under the history's hold, as [`Held::forget`].
    ///
    /// # Errors
    /// As [`Held::forget`].
    pub fn forget(&self, id: FactId, at: Datetime, by: &Actor) -> Result<Forgotten, Refusal> {
        self.hold()?.forget(id, at, by)
    }

    /// The sweep a write makes after it is written, under the history's hold: it never fails
    /// the write, which stands either way. A refused sweep (a store unreadable, or a read that
    /// left something out) is logged, and the next sweep deletes what this one did not, since
    /// every sweep deletes everything dead.
    pub fn sweep_after_write(&self) {
        match self.hold() {
            Ok(held) => held.sweep_after_write(),
            Err(refused) => refused_sweep(&refused),
        }
    }

    /// Sweep under the history's hold, as [`Held::sweep`].
    ///
    /// # Errors
    /// As [`Held::sweep`].
    pub fn sweep(&self) -> Result<usize, Refusal> {
        self.hold()?.sweep()
    }
}

impl Held<'_> {
    /// Remember a fact: its text kept first, then the event naming it, and the text removed again
    /// if the event is refused.
    ///
    /// # Errors
    /// [`Refusal::conflict`] at [`MAX_LIVE`] (naming the cap) and for a fact already standing
    /// word for word (naming its id); the salt's, the texts' and the history's own.
    pub fn remember(
        &self,
        fact: &Fact,
        kind: Kind,
        said: &Origin,
        at: Datetime,
    ) -> Result<Remembered, Refusal> {
        let memories = &self.memories;
        let history = memories.history()?;
        let id = remembering(&history, &memories.with_words(&history)?, fact)?;
        let text = Text::new(fact.clone(), self.salt()?);
        let name = text.name();
        let event = remembered(id, at, name.clone(), kind, said)?;
        memories.texts.set(&name, &text)?;
        if let Err(refused) = memories.history.append(event) {
            // Its words never stood, so they are not kept.
            let _ = memories.texts.remove(&name);
            return Err(refused.into());
        }
        Ok(Remembered {
            id,
            at,
            fact: fact.clone(),
            kind,
            said: said.clone(),
        })
    }

    /// Remember the words a card keeps under `text`: the fact names the card's text, so the two
    /// share it, and nothing is kept or removed here.
    ///
    /// # Errors
    /// [`Refusal::not_found`] for words no longer kept; otherwise as [`Held::remember`].
    pub fn remember_kept(
        &self,
        text: &TextName,
        kind: Kind,
        said: &Origin,
        at: Datetime,
    ) -> Result<Remembered, Refusal> {
        let memories = &self.memories;
        let fact = memories.words(text)?.ok_or_else(|| {
            Refusal::not_found("the card's words are deleted, so there is nothing to remember")
        })?;
        let history = memories.history()?;
        let id = remembering(&history, &memories.with_words(&history)?, &fact)?;
        memories
            .history
            .append(remembered(id, at, text.clone(), kind, said)?)?;
        Ok(Remembered {
            id,
            at,
            fact,
            kind,
            said: said.clone(),
        })
    }

    /// The words a remember card about `fact` names: those a card already keeps word for word,
    /// so the same fact asked again is the same card, or else the fact kept afresh.
    ///
    /// # Errors
    /// The salt's and the stores' own.
    pub fn keep_card(&self, fact: &Fact) -> Result<Kept, Refusal> {
        let memories = &self.memories;
        let fold = crate::secretary::inbox::fold(memories.cards)?;
        for object in fold.objects() {
            if let Proposal::Remember { text, .. } = &object.asked.proposal
                && memories.words(text)?.as_ref() == Some(fact)
            {
                return Ok(Kept::Found(text.clone()));
            }
        }
        let text = Text::new(fact.clone(), self.salt()?);
        let name = text.name();
        memories.texts.set(&name, &text)?;
        Ok(Kept::New(name))
    }

    /// Take back words kept for a card that was never filed; words a card already named stay.
    ///
    /// # Errors
    /// The texts' own.
    pub fn unkeep(&self, kept: &Kept) -> Result<(), Refusal> {
        match kept {
            Kept::New(name) => Ok(self.memories.texts.remove(name)?),
            Kept::Found(_) => Ok(()),
        }
    }

    /// Forget a standing fact: the event first, so the history says it is gone, and then the
    /// sweep after a write ([`Held::sweep_after_write`]), which never fails the forgetting.
    ///
    /// # Errors
    /// [`Refusal::not_found`] for an id no standing fact carries; the stores' own.
    pub fn forget(&self, id: FactId, at: Datetime, by: &Actor) -> Result<Forgotten, Refusal> {
        let memories = &self.memories;
        let version = forgetting(&memories.history()?, id)?;
        let event = memory_event::forgotten(id, at, by.clone(), version)
            .map_err(|refused| Refusal::internal(refused.to_string()))?;
        memories.history.append(event)?;
        // The forgetting stands whatever its erasures do; the next forgetting owes them again.
        if let Err(refused) = self.erase_forgotten() {
            refused_sweep(&refused);
        }
        self.sweep_after_write();
        Ok(Forgotten {
            id,
            at,
            by: by.as_str().to_owned(),
            version,
        })
    }

    /// Delete every text that something names and nothing live names: a forgotten fact's, a
    /// dismissed card's, a tapped card's whose fact is forgotten, a summary superseded or of a
    /// node the conversations' lines no longer name. Every naming is read before anything is
    /// deleted. Deleting one already gone
    /// succeeds, so a deletion an earlier sweep failed to make is made now. Answers how many
    /// texts it deleted or found gone.
    ///
    /// The one place words are deleted for what became of them, and asked under the hold, so a
    /// card filed or a fact remembered cannot come to name a text between the reading and the
    /// deletion. It reads every store whole or deletes nothing: an object a read left out may be
    /// the very one that keeps a text live.
    ///
    /// # Errors
    /// [`Refusal::conflict`] naming a store whose read left anything out; the stores' own.
    pub fn sweep(&self) -> Result<usize, Refusal> {
        let memories = &self.memories;
        whole(memories.history, "memory")?;
        whole(memories.cards, "inbox")?;
        whole(memories.digests.history, "digests")?;
        whole(memories.digests.lines.lines, "conversations")?;
        let facts = crate::secretary::texts::dead([
            memories.history()?.naming(),
            crate::secretary::inbox::fold(memories.cards)?.naming(),
        ]);
        let summaries = crate::secretary::texts::dead([memories.digests.naming()?]);
        for name in &facts {
            memories.texts.remove(name)?;
        }
        for name in &summaries {
            memories.digests.texts.remove(name)?;
        }
        Ok(facts.len() + summaries.len())
    }

    /// Erase from its conversation what every forgotten fact's remembering provably copied, and
    /// offer the owner what it cannot prove, a card in the same conversation: a function of the
    /// memory's history and the lines, each erasure stamped with its forgetting's instant, so a
    /// forgetting whose erasure failed is erased by the next, and one already erased owes
    /// nothing. Answers how many keys it erased.
    ///
    /// # Errors
    /// The stores' own; a conversation that does not read owes nothing until it does.
    pub fn erase_forgotten(&self) -> Result<usize, Refusal> {
        let history = self.memories.history()?;
        let lines = self.memories.digests.lines;
        let mut erased = 0;
        for (fact, gone) in history.forgotten() {
            let Some(kept) = history.remembered().get(fact) else {
                continue;
            };
            let conversation = kept.said.conversation();
            let Ok(transcript) = crate::secretary::transcript::read(lines, conversation) else {
                continue;
            };
            let caused = crate::secretary::caused::Caused::of(&transcript, *fact, &kept.said);
            if !caused.erased.is_empty() {
                erased += caused.erased.len();
                crate::secretary::transcript::erase(
                    lines,
                    conversation,
                    caused.erased,
                    crate::secretary::erasure::Erasure::Forgot { fact: *fact },
                    gone.at,
                )?;
            }
            if !caused.offered.is_empty() {
                self.offer(conversation, *fact, caused.offered, gone.at)?;
            }
        }
        Ok(erased)
    }

    /// The card offering `keys` of `conversation` for erasure, filed as the box's; the same
    /// offer filed again is the card already there.
    fn offer(
        &self,
        conversation: &crate::secretary::store::ConversationId,
        fact: FactId,
        keys: std::collections::BTreeSet<crate::secretary::locked_text::KeyName>,
        at: Datetime,
    ) -> Result<(), Refusal> {
        let lines = self.memories.digests.lines;
        // The card is its own evidence: the reading it rests on is the box's.
        let card = crate::reference::Reference::Proposal(crate::reference::ProposalRef {
            conversation: conversation.clone(),
            index: crate::secretary::read(lines, conversation)?.proposals.len(),
        });
        let reasons = crate::secretary::store::Reasons::new(
            format!(
                "forgetting fact {fact} erased the call that wrote it down; what was said around \
                 it may say it in other words, and only you can tell"
            ),
            vec![card],
        )
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
        crate::secretary::filed::file(
            lines,
            self.memories.cards,
            at,
            crate::secretary::filed::Filing {
                conversation,
                proposer: crate::secretary::proposer::Proposer::Box,
                proposal: Proposal::Act(crate::secretary::acts::Act::Erase { fact, keys }),
                reasons,
                serves: None,
                revises: None,
            },
        )?;
        Ok(())
    }

    /// [`Memories::sweep_after_write`], under this hold.
    pub fn sweep_after_write(&self) {
        if let Err(refused) = self.sweep() {
            refused_sweep(&refused);
        }
    }

    /// The digest, whose writes are made under this hold.
    pub(crate) fn digests(&self) -> Digests<'_> {
        self.memories.digests
    }

    pub(crate) fn salt(&self) -> Result<[u8; crate::secretary::salted_text::SALT_BYTES], Refusal> {
        self.memories
            .entropy
            .salt()
            .map_err(|failed| Refusal::internal(format!("no salt for the fact: {failed}")))
    }
}

/// A sweep after a write that could not run: said, and left to the next sweep.
fn refused_sweep(refused: &Refusal) {
    tracing::warn!(
        refused = refused.message(),
        "the sweep after a write was refused; the write stands and the next sweep deletes what \
         is dead"
    );
}

/// Refuse unless `store` read whole: a write that deletes, or decides over the reading, may not
/// rest on a read that left an object out.
///
/// # Errors
/// [`Refusal::conflict`] naming the store and what its read left out.
pub(crate) fn whole<E: prodrome_core::schema::Schema>(
    store: &dyn Prodrome<E>,
    store_name: &str,
) -> Result<(), Refusal> {
    let left_out = store.excluded()?;
    if left_out.is_empty() {
        return Ok(());
    }
    let names: Vec<&str> = left_out
        .iter()
        .map(prodrome_core::event::Hash::as_str)
        .collect();
    Err(Refusal::conflict(format!(
        "the {store_name} store's read left out {}: nothing is deleted or decided until it reads \
         whole",
        names.join(", ")
    )))
}

/// The event remembering fact `id`, whose words are kept under `text`.
///
/// # Errors
/// [`Refusal::internal`] for a refusal the schema never makes of a remembering.
fn remembered(
    id: FactId,
    at: Datetime,
    text: TextName,
    kind: Kind,
    said: &Origin,
) -> Result<MemoryEvent, Refusal> {
    memory_event::remembered(
        id,
        at,
        crate::secretary::writes_as(),
        text,
        kind,
        said.clone(),
    )
    .map_err(|refused| Refusal::internal(refused.to_string()))
}

/// Decide a remember: the new fact's number, one more than every id and version before it.
///
/// # Errors
/// [`Refusal::conflict`] for a fact standing word for word, naming its id, and at [`MAX_LIVE`]
/// standing facts, counted in the history so a fact whose text was lost still counts.
pub fn remembering(history: &TwoPhase, memory: &Memory, fact: &Fact) -> Result<FactId, Refusal> {
    if let Some(same) = memory.facts.iter().find(|standing| &standing.fact == fact) {
        return Err(Refusal::conflict(format!(
            "already remembered as fact {}",
            same.id
        )));
    }
    if history.live().count() >= MAX_LIVE {
        return Err(Refusal::conflict(format!(
            "the memory holds {MAX_LIVE} facts, its cap: forget one, or merge two into one, first"
        )));
    }
    Ok(FactId(history.version() + 1))
}

/// Decide a forget: the version it makes.
///
/// # Errors
/// [`Refusal::not_found`] for an id no standing fact carries.
pub fn forgetting(history: &TwoPhase, id: FactId) -> Result<u64, Refusal> {
    if !history.live().any(|(fact, _)| fact == id) {
        return Err(Refusal::not_found(format!(
            "no standing fact is numbered {id}"
        )));
    }
    Ok(history.version() + 1)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU8, Ordering};

    use super::*;
    use crate::secretary::inbox_event::InboxEvent;
    use crate::secretary::store::ConversationId;
    use crate::store::{InMemory, InMemoryDoc, Reads};

    /// Each salt one more than the last.
    #[derive(Default)]
    struct Counting(AtomicU8);

    /// A digest over no lines, in memory.
    #[derive(Default)]
    struct Digested {
        lines: crate::secretary::conversations::InMemory,
        history: InMemory<crate::secretary::digest_event::DigestEvent>,
        texts: InMemoryDoc<TextName, Text<crate::secretary::digest_line::Line>>,
        owners: std::collections::BTreeSet<Actor>,
    }

    impl Digested {
        fn digests(&self) -> Digests<'_> {
            Digests {
                history: &self.history,
                texts: &self.texts,
                lines: self.lines.conversations(),
                owners: &self.owners,
                compacting: &crate::secretary::compacting::Compacting::Off,
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
        let read = memories.read().expect("reads");
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
        let read = memories.read().expect("reads, never refuses");
        assert!(read.facts.is_empty());
        assert_eq!(read.version, 1);
        let history_read = memories.history().expect("reads");
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
    struct Failing<E: crate::store::Store> {
        inner: InMemory<E>,
        failing: std::sync::atomic::AtomicBool,
    }

    impl<E: crate::store::Store<Vocabulary: Default>> Failing<E> {
        fn new() -> Self {
            Self {
                inner: InMemory::default(),
                failing: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    impl<E: crate::store::Store> Reads<E> for Failing<E> {
        fn objects(
            &self,
        ) -> Result<
            Vec<(
                prodrome_core::event::Hash,
                prodrome_core::event::Envelope<E>,
            )>,
            crate::store::StoreError,
        > {
            if self.failing.load(Ordering::Relaxed) {
                return Err(crate::store::StoreError::unreadable("failing", "told to"));
            }
            self.inner.objects()
        }
        fn excluded(
            &self,
        ) -> Result<std::collections::BTreeSet<prodrome_core::event::Hash>, crate::store::StoreError>
        {
            if self.failing.load(Ordering::Relaxed) {
                return Err(crate::store::StoreError::unreadable("failing", "told to"));
            }
            self.inner.excluded()
        }
        fn tips(&self) -> Result<Vec<prodrome_core::event::Hash>, crate::store::StoreError> {
            self.inner.tips()
        }
        fn verify(&self) -> Vec<String> {
            self.inner.verify()
        }
    }

    impl<E: crate::store::Store> Prodrome<E> for Failing<E> {
        fn append(&self, event: E) -> Result<prodrome_core::event::Hash, crate::store::StoreError> {
            self.inner.append(event)
        }
        fn accept(
            &self,
            objects: &[(prodrome_core::event::Hash, String)],
        ) -> Result<Vec<prodrome_core::event::Hash>, crate::store::StoreError> {
            self.inner.accept(objects)
        }
        fn locked(&self) -> Result<Box<dyn WriteLock + '_>, crate::store::StoreError> {
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
            Failing::<crate::secretary::digest_event::DigestEvent>::new(),
            Failing::<crate::secretary::line_event::LineEvent>::new(),
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
                lines: crate::secretary::conversations::Conversations {
                    lines: &lines,
                    keys: &crate::store::InMemoryKeys::default(),
                    entropy: &entropy,
                    renders: &crate::capability::Nothing,
                    opened: &crate::secretary::opened::Opened::default(),
                },
                owners: &std::collections::BTreeSet::new(),
                compacting: &crate::secretary::compacting::Compacting::Off,
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
            assert_eq!(texts.keys().len(), 1, "and deletes nothing");
            failing.store(false, Ordering::Relaxed);
        }
        assert_eq!(memories.sweep().expect("sweeps"), 1);
        assert!(texts.keys().is_empty(), "read again, the dead text goes");
    }
}
