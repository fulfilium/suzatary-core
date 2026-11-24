//! The memory's two stores as one value: its history, a prodrome at the [`MemoryEvent`] schema
//! that names each fact's words, and its texts, which forgetting deletes.
//!
//! Every reading takes no lock; the non-monotone questions are the writes', asked under the
//! history's hold ([`Held`]).

use prodrome_data::event::Actor;
use prodrome_data::literal::Datetime;
use prodrome_data::schema::Schema;
use suzatary_lines::secretary::{conversations, transcript};

use crate::secretary::digests::{Digests, ReadDigests};
use suzatary_base::capability::Entropy;
use suzatary_base::refusal::Refusal;
use suzatary_inbox::secretary::inbox;
use suzatary_inbox::secretary::inbox_event::InboxEvent;
use suzatary_inbox::secretary::proposal::Proposal;
use suzatary_memory::secretary::memory::{
    Fact, FactId, Forgotten, Kind, MAX_LIVE, Memory, Origin, Remembered, Replaces, Sources,
};
use suzatary_memory::secretary::memory_event::{self, MemoryEvent};
use suzatary_memory::secretary::salted_text::{Text, TextName};
use suzatary_memory::secretary::texts;
use suzatary_memory::secretary::two_phase::TwoPhase;
use suzatary_store::store::{Gets, Lists, Prodrome, Reads, WriteLock};

#[derive(Clone, Copy)]
pub struct Memories<'a> {
    /// What was remembered and forgotten, by id; never a fact's words.
    pub history: &'a dyn Prodrome<MemoryEvent>,
    pub texts: &'a dyn Lists<TextName, Text<Fact>>,
    /// The inbox, whose remember cards name their words in [`Memories::texts`].
    pub cards: &'a dyn Prodrome<InboxEvent>,
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
    Found(TextName),
    /// Kept now, for a card not yet filed.
    New(TextName),
}

impl Kept {
    #[must_use]
    pub fn name(&self) -> &TextName {
        match self {
            Kept::Found(name) | Kept::New(name) => name,
        }
    }
}

/// The memory as a reader holds it: no field reaches a write.
#[derive(Clone, Copy)]
pub struct ReadMemories<'a> {
    pub history: &'a dyn Reads<MemoryEvent>,
    pub texts: &'a dyn Gets<TextName, Text<Fact>>,
    pub digests: ReadDigests<'a>,
}

impl ReadMemories<'_> {
    pub fn history(&self) -> Result<TwoPhase, Refusal> {
        Ok(TwoPhase::of(&self.history.nodes()?))
    }

    /// Every standing fact whose words read.
    ///
    /// # Errors
    /// A text that will not read is a failure, never an absent fact.
    pub fn read(&self) -> Result<Memory, Refusal> {
        self.with_words(&self.history()?)
    }

    /// The words kept under `name`, for a card or a fact; `None` once they are deleted.
    pub fn words(&self, name: &TextName) -> Result<Option<Fact>, Refusal> {
        Ok(self.texts.get(name)?.map(|text| text.words().clone()))
    }

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
}

impl<'a> Memories<'a> {
    #[must_use]
    pub fn reads(&self) -> ReadMemories<'a> {
        ReadMemories {
            history: self.history,
            texts: self.texts,
            digests: self.digests.reads(),
        }
    }

    pub fn hold(&self) -> Result<Held<'a>, Refusal> {
        Ok(Held {
            memories: *self,
            _hold: self.history.locked()?,
        })
    }

    pub fn remember(
        &self,
        fact: &Fact,
        kind: Kind,
        said: &Origin,
        at: Datetime,
    ) -> Result<Remembered, Refusal> {
        self.hold()?.remember(fact, kind, said, at)
    }

    pub fn consolidate(
        &self,
        from: &std::collections::BTreeSet<FactId>,
        fact: &Fact,
        kind: Kind,
        round: &Origin,
        at: Datetime,
    ) -> Result<Remembered, Refusal> {
        self.hold()?.consolidate(from, fact, kind, round, at)
    }

    pub fn forget(&self, id: FactId, at: Datetime, by: &Actor) -> Result<Forgotten, Refusal> {
        self.hold()?.forget(id, at, by)
    }

    /// The sweep after a write, under the hold: it never fails the write.
    pub fn sweep_after_write(&self) {
        match self.hold() {
            Ok(held) => held.sweep_after_write(),
            Err(refused) => refused_sweep(&refused),
        }
    }

    pub fn sweep(&self) -> Result<usize, Refusal> {
        suzatary_lines::parts::part("lock", || self.hold())?.sweep()
    }

    /// [`Memories::sweep`] of `which` alone, refused as the whole sweep is.
    pub fn sweep_only(&self, which: Texts) -> Result<usize, Refusal> {
        let held = suzatary_lines::parts::part("lock", || self.hold())?;
        held.wholes()?;
        held.dead(which)
    }
}

/// A fact's or a card's words are served while kept, a summary's only while live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Texts {
    Facts,
    Summaries,
}

impl Held<'_> {
    pub fn remember(
        &self,
        fact: &Fact,
        kind: Kind,
        said: &Origin,
        at: Datetime,
    ) -> Result<Remembered, Refusal> {
        self.write(fact, kind, &Replaces::Nothing, said, at)
    }

    /// Replace the standing facts `from` with `fact`, written in `round`.
    ///
    /// # Errors
    /// [`Refusal::bad_request`] for no source.
    pub fn consolidate(
        &self,
        from: &std::collections::BTreeSet<FactId>,
        fact: &Fact,
        kind: Kind,
        round: &Origin,
        at: Datetime,
    ) -> Result<Remembered, Refusal> {
        let from = Sources::new(from.clone())
            .ok_or_else(|| Refusal::bad_request("a consolidation replaces at least one fact"))?;
        self.write(fact, kind, &Replaces::Facts(from), round, at)
    }

    /// Write `fact` from `origin` in place of `replaces`: its words first, then the event, and
    /// the words removed again if the event is refused.
    pub fn write(
        &self,
        fact: &Fact,
        kind: Kind,
        replaces: &Replaces,
        origin: &Origin,
        at: Datetime,
    ) -> Result<Remembered, Refusal> {
        let text = Text::new(fact.clone(), self.salt()?);
        let name = text.name();
        self.memories.texts.set(&name, &text)?;
        let appended = self
            .event(fact, &name, (kind, replaces, origin, at))
            .and_then(|event| {
                let id = *Schema::key(&event);
                self.memories.history.append(event)?;
                Ok(id)
            });
        match appended {
            Ok(id) => self.written(id, fact, kind, replaces, at),
            Err(refused) => {
                let _ = self.memories.texts.remove(&name);
                Err(refused)
            }
        }
    }

    /// Write the words a tapped card keeps under `text`; the fact and the card share the text.
    ///
    /// # Errors
    /// [`Refusal::not_found`] for words no longer kept.
    pub fn write_kept(
        &self,
        text: &TextName,
        kind: Kind,
        replaces: &Replaces,
        origin: &Origin,
        at: Datetime,
    ) -> Result<Remembered, Refusal> {
        let fact = self.memories.reads().words(text)?.ok_or_else(|| {
            Refusal::not_found("the card's words are deleted, so there is nothing to remember")
        })?;
        let event = self.event(&fact, text, (kind, replaces, origin, at))?;
        let id = *Schema::key(&event);
        self.memories.history.append(event)?;
        self.written(id, &fact, kind, replaces, at)
    }

    /// The event writing `fact` under the next number, its words kept under `text`.
    ///
    /// # Errors
    /// [`Refusal::conflict`] at [`MAX_LIVE`] or for a duplicate standing fact;
    /// [`Refusal::not_found`] for a source that does not stand.
    fn event(
        &self,
        fact: &Fact,
        text: &TextName,
        (kind, replaces, origin, at): (Kind, &Replaces, &Origin, Datetime),
    ) -> Result<MemoryEvent, Refusal> {
        let reads = self.memories.reads();
        let history = reads.history()?;
        let standing = reads.with_words(&history)?;
        match replaces {
            Replaces::Nothing => remembered(
                remembering(&history, &standing, fact)?,
                at,
                text.clone(),
                kind,
                origin,
            ),
            Replaces::Facts(from) => memory_event::consolidated(
                consolidating(&history, &standing, from.ids(), fact)?,
                at,
                transcript::writer(),
                from.ids().clone(),
                text.clone(),
                kind,
                origin.clone(),
            )
            .map_err(|refused| Refusal::internal(refused.to_string())),
        }
    }

    /// After a consolidation, the sweep deletes its sources' words.
    fn written(
        &self,
        id: FactId,
        fact: &Fact,
        kind: Kind,
        replaces: &Replaces,
        at: Datetime,
    ) -> Result<Remembered, Refusal> {
        match replaces {
            Replaces::Nothing => {}
            Replaces::Facts(_) => self.sweep_after_write(),
        }
        let said = self
            .memories
            .reads()
            .history()?
            .provenance(id)
            .cloned()
            .ok_or_else(|| {
                Refusal::internal(format!("fact {id} was written and does not read back"))
            })?;
        Ok(Remembered {
            id,
            at,
            fact: fact.clone(),
            kind,
            said,
        })
    }

    /// The words a remember card about `fact` names: a card's already, word for word, or else
    /// kept afresh.
    pub fn keep_card(&self, fact: &Fact) -> Result<Kept, Refusal> {
        let memories = &self.memories;
        let fold = inbox::fold(memories.cards)?;
        for object in fold.objects() {
            if let Proposal::Remember { text, .. } = &object.asked.proposal
                && memories.reads().words(text)?.as_ref() == Some(fact)
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
    pub fn unkeep(&self, kept: &Kept) -> Result<(), Refusal> {
        match kept {
            Kept::New(name) => Ok(self.memories.texts.remove(name)?),
            Kept::Found(_) => Ok(()),
        }
    }

    /// Forget a standing fact: the event first, then the sweep, which never fails the forgetting.
    ///
    /// # Errors
    /// [`Refusal::not_found`] for an id no standing fact carries.
    pub fn forget(&self, id: FactId, at: Datetime, by: &Actor) -> Result<Forgotten, Refusal> {
        let memories = &self.memories;
        let version = forgetting(&memories.reads().history()?, id)?;
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

    /// Delete every held text that something names and nothing live names, and answer how many.
    ///
    /// It reads every store whole or deletes nothing, since an object left out may be the one
    /// that keeps a text live. It is asked under the hold, so nothing can come to name a text
    /// between the reading and the deletion.
    ///
    /// # Errors
    /// [`Refusal::conflict`] naming a store whose read left anything out.
    pub fn sweep(&self) -> Result<usize, Refusal> {
        self.wholes()?;
        Ok(self.dead(Texts::Facts)? + self.dead(Texts::Summaries)?)
    }

    fn wholes(&self) -> Result<(), Refusal> {
        use suzatary_lines::parts::part;
        let memories = &self.memories;
        part("whole.memory", || whole(memories.history, "memory"))?;
        part("whole.inbox", || whole(memories.cards, "inbox"))?;
        part("whole.digests", || {
            whole(memories.digests.history, "digests")
        })?;
        part("whole.lines", || {
            whole(memories.digests.lines.lines, "conversations")
        })
    }

    /// Delete the dead texts of `which` its store holds, and answer how many.
    fn dead(&self, which: Texts) -> Result<usize, Refusal> {
        use suzatary_lines::parts::part;
        let memories = &self.memories;
        let gone = match which {
            Texts::Facts => held_dead(
                [
                    part("facts", || memories.reads().history())?.naming(),
                    part("cards", || inbox::fold(memories.cards))?.naming(),
                ],
                part("held", || memories.texts.held()),
            ),
            Texts::Summaries => held_dead(
                [part("summaries", || memories.digests.reads().naming())?],
                part("held", || memories.digests.texts.held()),
            ),
        };
        for name in &gone {
            part("delete", || match which {
                Texts::Facts => memories.texts.remove(name),
                Texts::Summaries => memories.digests.texts.remove(name),
            })?;
        }
        Ok(gone.len())
    }

    /// Erase from its conversation what every forgotten fact's writing provably copied, and offer
    /// what it cannot prove as a card. Answers how many keys it erased. Each erasure is stamped
    /// with its forgetting's instant, so a failed one is erased by the next; a forgetting takes
    /// what its facts' sources wrote too ([`TwoPhase::forgotten_with`]).
    ///
    /// # Errors
    /// A conversation that does not read owes nothing until it does.
    pub fn erase_forgotten(&self) -> Result<usize, Refusal> {
        let history = self.memories.reads().history()?;
        let lines = self.memories.digests.lines;
        let mut erased = 0;
        for (fact, gone) in history.forgotten() {
            for (written, round) in history.forgotten_with(*fact) {
                let conversation = round.conversation();
                let Ok(transcript) =
                    suzatary_lines::secretary::transcript::read(lines, conversation)
                else {
                    continue;
                };
                let caused = crate::secretary::caused::Caused::of(&transcript, written, round);
                if !caused.erased.is_empty() {
                    erased += caused.erased.len();
                    suzatary_lines::secretary::transcript::erase(
                        lines,
                        conversation,
                        caused.erased,
                        suzatary_memory::secretary::erasure::Erasure::Forgot { fact: *fact },
                        gone.at,
                    )?;
                }
                if !caused.offered.is_empty() {
                    self.offer(conversation, *fact, caused.offered, gone.at)?;
                }
            }
        }
        Ok(erased)
    }

    /// The card offering `keys` for erasure, filed as the box's; filed again, the same card.
    fn offer(
        &self,
        conversation: &suzatary_base::conversation_id::ConversationId,
        fact: FactId,
        keys: std::collections::BTreeSet<suzatary_memory::secretary::locked_text::KeyName>,
        at: Datetime,
    ) -> Result<(), Refusal> {
        let lines = self.memories.digests.lines;
        // The card is its own evidence: the reading it rests on is the box's.
        let card =
            suzatary_base::reference::Reference::Proposal(suzatary_base::reference::ProposalRef {
                conversation: conversation.clone(),
                index: conversations::read(lines, conversation)?.proposals.len(),
            });
        let reasons = suzatary_inbox::secretary::proposal::Reasons::new(
            format!(
                "forgetting fact {fact} erased the call that wrote it down; what was said around \
                 it may say it in other words, and only you can tell"
            ),
            vec![card],
        )
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
        suzatary_topics::secretary::filed::file(
            lines,
            self.memories.cards,
            &[],
            at,
            suzatary_topics::secretary::filed::Filing {
                conversation,
                proposer: suzatary_inbox::secretary::proposer::Proposer::Box,
                proposal: Proposal::Act(suzatary_inbox::secretary::acts::Act::Erase { fact, keys }),
                reasons,
                serves: None,
                revises: None,
            },
        )?;
        Ok(())
    }

    pub fn sweep_after_write(&self) {
        if let Err(refused) = self.sweep() {
            refused_sweep(&refused);
        }
    }

    pub(crate) fn digests(&self) -> Digests<'_> {
        self.memories.digests
    }

    pub(crate) fn salt(&self) -> Result<[u8; suzatary_base::capability::SALT_BYTES], Refusal> {
        self.memories
            .entropy
            .salt()
            .map_err(|failed| Refusal::internal(format!("no salt for the fact: {failed}")))
    }
}

/// The dead texts of `namings` among those `held`; a store that cannot list them is asked to
/// remove every dead name.
fn held_dead<T>(
    namings: impl IntoIterator<Item = texts::Naming<T>>,
    held: Result<std::collections::BTreeSet<TextName>, suzatary_store::store::StoreError>,
) -> std::collections::BTreeSet<TextName> {
    match held {
        Ok(held) => texts::gone(namings, &held),
        Err(unlisted) => {
            tracing::warn!(
                "the sweep removes every dead name: its store did not list ({unlisted})"
            );
            texts::dead(namings)
        }
    }
}

fn refused_sweep(refused: &Refusal) {
    tracing::warn!(
        refused = refused.message(),
        "the sweep after a write was refused; the write stands and the next sweep deletes what \
         is dead"
    );
}

/// Refuse unless `store` read whole: a deciding write may not rest on a partial read.
pub(crate) fn whole<E: Schema>(store: &dyn Reads<E>, store_name: &str) -> Result<(), Refusal> {
    let left_out = store.excluded()?;
    if left_out.is_empty() {
        return Ok(());
    }
    let names: Vec<&str> = left_out
        .iter()
        .map(prodrome_data::event::Hash::as_str)
        .collect();
    Err(Refusal::conflict(format!(
        "the {store_name} store's read left out {}: nothing is deleted or decided until it reads \
         whole",
        names.join(", ")
    )))
}

fn remembered(
    id: FactId,
    at: Datetime,
    text: TextName,
    kind: Kind,
    said: &Origin,
) -> Result<MemoryEvent, Refusal> {
    memory_event::remembered(id, at, transcript::writer(), text, kind, said.clone())
        .map_err(|refused| Refusal::internal(refused.to_string()))
}

/// Decide a remember: the new fact's number.
///
/// # Errors
/// [`Refusal::conflict`] for a duplicate standing fact, and at [`MAX_LIVE`] counted in the
/// history, so a fact whose text was lost still counts.
pub fn remembering(history: &TwoPhase, memory: &Memory, fact: &Fact) -> Result<FactId, Refusal> {
    if let Some(same) = memory.facts.iter().find(|standing| &standing.fact == fact) {
        return Err(Refusal::conflict(format!(
            "already remembered as fact {}",
            same.id
        )));
    }
    if history.live().count() >= MAX_LIVE {
        return Err(Refusal::conflict(format!(
            "the memory holds {MAX_LIVE} facts, its cap: forget one, or consolidate several into one, first"
        )));
    }
    Ok(FactId(history.version() + 1))
}

/// Decide a consolidation: the new fact's number. Its words may repeat a source's, never
/// another standing fact's.
///
/// # Errors
/// [`Refusal::not_found`] for a source that does not stand; [`Refusal::conflict`] for a duplicate.
pub fn consolidating(
    history: &TwoPhase,
    memory: &Memory,
    from: &std::collections::BTreeSet<FactId>,
    fact: &Fact,
) -> Result<FactId, Refusal> {
    let standing: std::collections::BTreeSet<FactId> = history.live().map(|(id, _)| id).collect();
    if let Some(gone) = from.iter().find(|id| !standing.contains(id)) {
        return Err(Refusal::not_found(format!(
            "no standing fact is numbered {gone}"
        )));
    }
    if let Some(same) = memory
        .facts
        .iter()
        .find(|other| &other.fact == fact && !from.contains(&other.id))
    {
        return Err(Refusal::conflict(format!(
            "already remembered as fact {}",
            same.id
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
