//! The conversations as one prodrome at the [`LineEvent`] schema, keyed by conversation: each
//! line a write to its conversation's transcript, resting on the lines its writer held, so the
//! lines are causally ordered and two writers that did not see each other make two heads. Each
//! line is read with its words opened by the key store; a slot whose key an erasure line of the
//! same conversation names reads as erased, and one whose key is not held reads as missing.
//!
//! [`Transcript`] is one conversation's lines in that order, ties between lines nothing orders
//! broken by the instant stamped and then the name, as undo's stream breaks them. The plan's
//! three registers are reads of it: the transcript itself ([`Transcript::stored`], the fold the
//! page draws), [`Transcript::owed`] (what a round still answers) and the proposal lines
//! against the inbox ([`Stored::lines`], each placed or named as unplaced).
//!
//! Every line is written through [`push`] or [`start`], which take its words through the
//! redaction door and lock each slot under a fresh key before the store sees it: nothing enters
//! the lines with its words in the clear.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use prodrome_core::event::{Actor, Envelope, Hash};
use prodrome_core::literal::Datetime;
use prodrome_core::registers::Node;

use crate::secretary::conversations::Conversations;
use crate::secretary::erasure::Erasure;
use crate::secretary::line_event::LineEvent;
use crate::secretary::line_shape::LineShape;
use crate::secretary::line_words::LineWords;
use crate::secretary::locked_text::{KeyName, TextKey};
use crate::secretary::slots::Slots;
use crate::secretary::store::{ConversationId, Entry, NotAConversation, Said, Stored};
use crate::secretary::words::{Gone, Words};
use crate::store::{Prodrome, StoreError};

/// One conversation's lines in causal order, and their fold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    lines: Vec<Written>,
    stored: Stored,
    /// Each turn's line, by the turn's index in [`Stored::turns`].
    turns: Vec<usize>,
}

/// One line beside its name, its reading and the lines it rests on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// The object's name.
    pub name: Hash,
    /// The line as stored.
    pub line: LineEvent,
    /// The line as read, its words opened or gone.
    pub entry: Entry,
    /// The positions, in [`Transcript::lines`], of the lines it rests on directly.
    pub rests_on: Vec<usize>,
}

/// One line of one conversation as a node holds it: its name, the line, and the names of the
/// objects it rests on, which may include objects that are no line of the conversation.
#[derive(Debug, Clone, Copy)]
pub struct Write<'n> {
    /// The object's name.
    pub name: &'n Hash,
    /// The line.
    pub line: &'n LineEvent,
    /// The names of the objects it rests on.
    pub parents: &'n [Hash],
}

impl Transcript {
    /// One conversation's writes, in any order, put in causal order, each opened by the key
    /// `slots` answers for its name, and folded.
    ///
    /// # Errors
    /// [`NotAConversation`] for lines the fold refuses.
    pub fn of(
        id: &ConversationId,
        writes: &[Write<'_>],
        slots: &impl Slots,
    ) -> Result<Self, NotAConversation> {
        let order = causal(
            &writes
                .iter()
                .map(|write| (write.name, write.line.at(), write.parents))
                .collect::<Vec<_>>(),
        );
        let erased = erasures(
            order
                .iter()
                .map(|(at, _)| (writes[*at].name, writes[*at].line)),
        );
        let read = Through {
            erased: &erased,
            slots,
        };
        let lines: Vec<Written> = order
            .into_iter()
            .map(|(at, rests_on)| Written {
                name: writes[at].name.clone(),
                line: writes[at].line.clone(),
                entry: writes[at].line.entry(&read),
                rests_on,
            })
            .collect();
        let entries: Vec<Entry> = lines.iter().map(|w| w.entry.clone()).collect();
        let stored = Stored::of(id, &entries)?;
        // The fold makes a turn of each said line and each stop, and of nothing else.
        let turns = entries
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, entry)| matches!(entry, Entry::Said { .. } | Entry::Stopped { .. }))
            .map(|(at, _)| at)
            .collect();
        Ok(Self {
            lines,
            stored,
            turns,
        })
    }

    /// Every line `line` rests on, directly or not, by its position in [`Transcript::lines`].
    #[must_use]
    pub fn beneath(&self, line: usize) -> std::collections::BTreeSet<usize> {
        let mut found = std::collections::BTreeSet::new();
        let mut pending = self.lines[line].rests_on.clone();
        while let Some(earlier) = pending.pop() {
            if found.insert(earlier) {
                pending.extend(&self.lines[earlier].rests_on);
            }
        }
        found
    }

    /// Every line, in causal order.
    #[must_use]
    pub fn lines(&self) -> &[Written] {
        &self.lines
    }

    /// The line of turn `turn`, by its position in [`Transcript::lines`].
    #[must_use]
    pub fn line_of(&self, turn: usize) -> Option<usize> {
        self.turns.get(turn).copied()
    }

    /// Every line's entry, in causal order: what [`Stored::of`] folds.
    #[must_use]
    pub fn entries(&self) -> Vec<Entry> {
        self.lines
            .iter()
            .map(|written| written.entry.clone())
            .collect()
    }

    /// The transcript register: the lines folded.
    #[must_use]
    pub fn stored(&self) -> &Stored {
        &self.stored
    }

    /// The transcript register, owned.
    #[must_use]
    pub fn into_stored(self) -> Stored {
        self.stored
    }

    /// The owed register: every live turn that no live closing turn rests on or is, with its
    /// index; a turn [closes](Stored::closes) when it is an answer with no calls or a stop.
    ///
    /// For one writer this is [`crate::secretary::queue::delta`]. Where two writers did not see
    /// each other, a question the answer or the stop did not rest on stays owed, wherever the
    /// order put it. Since it reads descent and not position, it is the same for every order a
    /// replica received the lines in, and one write closes a question for good.
    #[must_use]
    pub fn owed(&self) -> Vec<(usize, &Said)> {
        let mut settled = vec![false; self.lines.len()];
        let mut pending: Vec<usize> = self
            .turns
            .iter()
            .enumerate()
            .filter(|(turn, _)| self.stored.is_live(*turn) && self.stored.closes(*turn))
            .map(|(_, &line)| line)
            .collect();
        while let Some(line) = pending.pop() {
            if !settled[line] {
                settled[line] = true;
                pending.extend(&self.lines[line].rests_on);
            }
        }
        self.stored
            .live()
            .into_iter()
            .filter(|(turn, _)| !settled[self.turns[*turn]])
            .collect()
    }
}

/// A conversation's reading is named by its heads, the lines no other line rests on: the down-set
/// they close is every line, so two replicas holding the same lines name one reading.
impl crate::rule::Named for Transcript {
    fn name(&self) -> crate::rule::ReadingName {
        let rested_on: std::collections::BTreeSet<usize> = self
            .lines
            .iter()
            .flat_map(|written| written.rests_on.iter().copied())
            .collect();
        crate::rule::ReadingName::of(
            self.lines
                .iter()
                .enumerate()
                .filter(|(at, _)| !rested_on.contains(at))
                .map(|(_, written)| written.name.as_str()),
        )
    }
}

/// Every key the erasure lines among `lines` name, each with why: where two name one key, the
/// erasure of the least object name says, so the answer is a function of the set of lines.
#[must_use]
pub fn erasures<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
) -> BTreeMap<KeyName, Erasure> {
    erasers(lines)
        .into_iter()
        .map(|(key, (_, erasure))| (key, erasure.clone()))
        .collect()
}

/// [`erasures`], each beside the name of the erasure line that says why.
#[must_use]
pub fn erasers<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
) -> BTreeMap<KeyName, (&'l Hash, &'l Erasure)> {
    let mut named: BTreeMap<KeyName, (&Hash, &Erasure)> = BTreeMap::new();
    for (name, line) in lines {
        if let LineShape::Erased { keys, erasure } = line.shape() {
            for key in keys {
                let at = named.entry(key.clone()).or_insert((name, erasure));
                if name < at.0 {
                    *at = (name, erasure);
                }
            }
        }
    }
    named
}

/// A slot as one conversation reads it: erased where `erased` names its key, before anything
/// kept or held is asked, else as `slots` reads it.
struct Through<'a, S> {
    erased: &'a BTreeMap<KeyName, Erasure>,
    slots: &'a S,
}

impl<S: Slots> Slots for Through<'_, S> {
    fn words(
        &self,
        name: &KeyName,
        binding: &[u8],
        sealed: &[u8],
        open: &dyn Fn(&TextKey) -> Option<LineWords>,
    ) -> Words<LineWords> {
        match self.erased.get(name) {
            Some(erasure) => Gone::Erased {
                erasure: erasure.clone(),
            }
            .into(),
            None => self.slots.words(name, binding, sealed, open),
        }
    }
}

/// Lines, by conversation.
fn by_conversation<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
) -> BTreeMap<&'l ConversationId, Vec<(&'l Hash, &'l LineEvent)>> {
    let mut by: BTreeMap<&ConversationId, Vec<(&Hash, &LineEvent)>> = BTreeMap::new();
    for (name, line) in lines {
        by.entry(line.conversation())
            .or_default()
            .push((name, line));
    }
    by
}

/// The objects' lines, by name.
fn lines_of(objects: &[(Hash, Envelope<LineEvent>)]) -> impl Iterator<Item = (&Hash, &LineEvent)> {
    objects
        .iter()
        .filter_map(|(name, object)| object.event().map(|line| (name, line)))
}

/// Every slot whose key an erasure line of its own conversation names, as the line, the slot and
/// its key's name: what a sweep destroys, on the box or in a tab. A function of the set of lines,
/// and monotone in it, since a key an erasure names stays named as lines are added.
#[must_use]
pub fn dead<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
) -> Vec<(&'l LineEvent, usize, &'l KeyName)> {
    by_conversation(lines)
        .into_values()
        .flat_map(|lines| {
            let erased = erasures(lines.iter().copied());
            lines
                .into_iter()
                .flat_map(|(_, line)| {
                    line.keys()
                        .enumerate()
                        .map(move |(slot, key)| (line, slot, key))
                })
                .filter(|(_, _, key)| erased.contains_key(*key))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The answer blocks slot `slot`'s words were rendered to, opened by `key`: what a replica
/// forgets before it destroys the key, since nothing finds them once the words are gone.
#[must_use]
pub fn fed(
    line: &LineEvent,
    slot: usize,
    key: &TextKey,
) -> Vec<crate::secretary::answer::Renderable> {
    match line.open(slot, key) {
        Some(LineWords::Model { text }) => crate::secretary::answer::blocks(&text)
            .iter()
            .map(|block| crate::secretary::answer::Renderable::of(&block.source))
            .collect(),
        _ => Vec::new(),
    }
}

/// Every key the objects' lines name that an erasure of the same conversation names: the keys
/// whose words are erased, each with why. A key an erasure names in another conversation is no
/// key of its, and erases nothing.
#[must_use]
pub fn erased(objects: &[(Hash, Envelope<LineEvent>)]) -> BTreeMap<KeyName, Erasure> {
    by_conversation(lines_of(objects))
        .into_values()
        .flat_map(|lines| {
            let named: std::collections::BTreeSet<&KeyName> =
                lines.iter().flat_map(|(_, line)| line.keys()).collect();
            erasures(lines.iter().copied())
                .into_iter()
                .filter(|(key, _)| named.contains(key))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Every line the objects hold, by name, read: each slot opened by the key `key` answers, or
/// erased where an erasure line of its own conversation names its key.
#[must_use]
pub fn opened(
    objects: &[(Hash, Envelope<LineEvent>)],
    slots: &impl Slots,
) -> BTreeMap<Hash, Entry> {
    by_conversation(lines_of(objects))
        .into_values()
        .flat_map(|lines| {
            let erased = erasures(lines.iter().copied());
            let read = Through {
                erased: &erased,
                slots,
            };
            lines
                .into_iter()
                .map(|(name, line)| (name.clone(), line.entry(&read)))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Kahn's order over one conversation's writes, each `(name, at, parents)`, the least `(at, name)`
/// among those ready first, each with the positions in the order of the writes it rests on.
///
/// A write is ready once every parent that is a write of the conversation is placed, so the cost
/// is the writes and their edges: a parent that is no write here (the genesis) orders nothing.
/// The order is a function of the writes, whatever order they are given in.
pub(crate) fn causal(writes: &[(&Hash, Datetime, &[Hash])]) -> Vec<(usize, Vec<usize>)> {
    let index: BTreeMap<&Hash, usize> = writes
        .iter()
        .enumerate()
        .map(|(at, (name, _, _))| (*name, at))
        .collect();
    let parents: Vec<Vec<usize>> = writes
        .iter()
        .map(|(_, _, parents)| {
            parents
                .iter()
                .filter_map(|p| index.get(p).copied())
                .collect()
        })
        .collect();
    let mut waiting: Vec<usize> = parents.iter().map(Vec::len).collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); writes.len()];
    for (later, earlier) in parents.iter().enumerate() {
        for &one in earlier {
            children[one].push(later);
        }
    }
    let key = |at: usize| Reverse((writes[at].1, writes[at].0.clone(), at));
    let mut ready: BinaryHeap<Reverse<(Datetime, Hash, usize)>> = (0..writes.len())
        .filter(|&at| waiting[at] == 0)
        .map(key)
        .collect();
    let mut placed = vec![usize::MAX; writes.len()];
    let mut order = Vec::with_capacity(writes.len());
    while let Some(Reverse((_, _, at))) = ready.pop() {
        for &next in &children[at] {
            waiting[next] -= 1;
            if waiting[next] == 0 {
                ready.push(key(next));
            }
        }
        placed[at] = order.len();
        order.push(at);
    }
    order
        .into_iter()
        .map(|at| {
            let mut rests_on: Vec<usize> = parents[at].iter().map(|&p| placed[p]).collect();
            rests_on.sort_unstable();
            (at, rests_on)
        })
        .collect()
}

/// The nodes' lines, by conversation, each with the objects it rests on.
fn writes(nodes: &[Node<LineEvent>]) -> BTreeMap<&ConversationId, Vec<Write<'_>>> {
    let mut by: BTreeMap<&ConversationId, Vec<Write<'_>>> = BTreeMap::new();
    for node in nodes {
        if let Some(line) = node.event() {
            by.entry(line.conversation()).or_default().push(Write {
                name: node.name(),
                line,
                parents: node.parents(),
            });
        }
    }
    by
}

/// Every conversation the nodes hold, read, each slot opened by the key `key` answers.
///
/// # Errors
/// [`NotAConversation`] for lines the fold refuses.
pub fn transcripts(
    nodes: &[Node<LineEvent>],
    slots: &impl Slots,
) -> Result<BTreeMap<ConversationId, Transcript>, NotAConversation> {
    writes(nodes)
        .into_iter()
        .map(|(id, writes)| Ok((id.clone(), Transcript::of(id, &writes, slots)?)))
        .collect()
}

/// Every conversation the objects hold, read, whatever order they are handed over in.
///
/// # Errors
/// [`ReadFailed`]: objects that are no prodrome, or lines the fold refuses.
pub fn of_objects(
    objects: impl IntoIterator<Item = (Hash, Envelope<LineEvent>)>,
    slots: &impl Slots,
) -> Result<BTreeMap<ConversationId, Transcript>, ReadFailed> {
    let nodes = prodrome_core::dag::Dag::from_iter(objects)
        .nodes()
        .map_err(|failed| ReadFailed::Unreadable(failed.to_string()))?;
    Ok(transcripts(&nodes, slots)?)
}

/// A store whose conversations do not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadFailed {
    /// Its objects are no prodrome.
    #[error("the conversations do not read: {0}")]
    Unreadable(String),
    /// A conversation's lines the fold refuses.
    #[error(transparent)]
    Unfolded(#[from] NotAConversation),
}

fn unreadable(failed: impl std::fmt::Display) -> StoreError {
    StoreError::unreadable("conversation store", failed)
}

/// Every conversation, read.
///
/// # Errors
/// The store's own, and [`StoreError::Unreadable`] for lines the fold refuses.
pub fn all(
    conversations: Conversations<'_>,
) -> Result<BTreeMap<ConversationId, Transcript>, StoreError> {
    transcripts(&conversations.lines.nodes()?, &conversations.slots()).map_err(unreadable)
}

/// One conversation's reading, or why its lines are none.
pub type Read = Result<Transcript, NotAConversation>;

/// Every conversation, newest id first, each read on its own, so one whose lines the fold refuses
/// is that conversation's refusal and no other's: one fold of the store for all of them.
///
/// # Errors
/// The store's own.
pub fn each(conversations: Conversations<'_>) -> Result<Vec<(ConversationId, Read)>, StoreError> {
    each_of(conversations.lines, &conversations.slots())
}

fn each_of(
    lines: &dyn Prodrome<LineEvent>,
    slots: &impl Slots,
) -> Result<Vec<(ConversationId, Read)>, StoreError> {
    let nodes = lines.nodes()?;
    let mut read: Vec<_> = writes(&nodes)
        .into_iter()
        .map(|(id, writes)| (id.clone(), Transcript::of(id, &writes, slots)))
        .collect();
    read.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(read)
}

/// What `verify` reports of the conversations, read against `inbox` and by their shapes alone,
/// since whether a conversation folds never rests on its words: the store's own findings,
/// every conversation whose lines do not fold, and every proposal line naming a record the inbox
/// does not hold, by its reference. An inbox that does not fold, and a line answered and never
/// asked, are the inbox's own findings ([`crate::secretary::inbox::findings`]), so each is named
/// once.
#[must_use]
pub fn findings(
    lines: &dyn Prodrome<LineEvent>,
    inbox: &dyn Prodrome<crate::secretary::inbox_event::InboxEvent>,
) -> Vec<String> {
    let mut found = lines.verify();
    let inbox = crate::secretary::inbox::fold(inbox).ok();
    let read = match each_of(lines, &|_: &KeyName| -> Words<TextKey> {
        Gone::Missing.into()
    }) {
        Ok(read) => read,
        Err(failed) => {
            found.push(failed.to_string());
            return found;
        }
    };
    for (id, read) in read {
        let stored = match read {
            Ok(transcript) => transcript.into_stored(),
            Err(why) => {
                found.push(format!("conversation {id} does not read: {why}"));
                continue;
            }
        };
        let Some(inbox) = &inbox else { continue };
        found.extend(
            stored
                .lines(inbox)
                .unplaced()
                .filter(|unplaced| unplaced.why == crate::secretary::store::Unheld::NotHeld)
                .map(|unplaced| {
                    let line =
                        crate::reference::Reference::Proposal(crate::reference::ProposalRef {
                            conversation: stored.id.clone(),
                            index: unplaced.asking.index,
                        });
                    format!("{line} has no card: {}", unplaced.why)
                }),
        );
    }
    found
}

/// One conversation, read.
///
/// # Errors
/// [`StoreError::Absent`] for an id never started, and [`all`]'s.
pub fn read(
    conversations: Conversations<'_>,
    id: &ConversationId,
) -> Result<Transcript, StoreError> {
    let nodes = conversations.lines.nodes()?;
    let writes = writes(&nodes)
        .remove(id)
        .ok_or_else(|| StoreError::Absent {
            what: "conversation",
            name: id.to_string(),
        })?;
    Transcript::of(id, &writes, &conversations.slots()).map_err(unreadable)
}

/// One conversation's lines, in causal order.
///
/// # Errors
/// As [`read`].
pub fn lines(
    conversations: Conversations<'_>,
    id: &ConversationId,
) -> Result<Vec<Entry>, StoreError> {
    Ok(read(conversations, id)?.entries())
}

/// Every conversation's id, sorted.
///
/// # Errors
/// The store's.
pub fn keys(conversations: Conversations<'_>) -> Result<Vec<ConversationId>, StoreError> {
    let nodes = conversations.lines.nodes()?;
    Ok(writes(&nodes).into_keys().cloned().collect())
}

/// Destroy every key an erasure line names that the key store still holds, each once the blocks
/// its words were rendered to are forgotten, and say how many. A key an erasure names stays named
/// as lines are added, so the sweep is monotone: it takes no hold, and running it twice, or on two
/// replicas, destroys what running it once does.
///
/// # Errors
/// The stores' own, and [`StoreError::Unwritable`] naming every key kept because a block
/// rendered from its words may still be on disk; the next sweep tries again.
pub fn sweep(conversations: Conversations<'_>) -> Result<usize, StoreError> {
    let objects = conversations.lines.objects()?;
    let mut destroyed = 0;
    let mut kept = Vec::new();
    for (line, slot, name) in dead(lines_of(&objects)) {
        conversations.opened.forget(name);
        let Some(key) = conversations.keys.get(name)? else {
            continue;
        };
        // A copy that may still be on disk keeps the key, so the next sweep can find the blocks
        // again and forget them; destroying it first would lose them for good.
        if let Err(failed) = fed(line, slot, &key)
            .iter()
            .try_for_each(|block| conversations.renders.forget(block))
        {
            kept.push(format!("{name}: {failed}"));
            continue;
        }
        conversations.keys.remove(name)?;
        // A read between the first forgetting and the removal may have kept the words again.
        conversations.opened.forget(name);
        destroyed += 1;
    }
    if kept.is_empty() {
        Ok(destroyed)
    } else {
        Err(StoreError::unwritable(
            "render cache",
            format!(
                "kept {} key(s) whose rendered blocks may still be on disk: {}",
                kept.len(),
                kept.join("; ")
            ),
        ))
    }
}

/// Erase `keys`' words from conversation `id`: one erasure line naming them, then the sweep, whose
/// count it answers (a sweep that kept keys is logged and counts nothing).
///
/// # Errors
/// [`StoreError::NotAppended`] for no keys, or a key no line of `id` holds words under (an erasure
/// erases only its own conversation's words), and [`push`]'s.
pub fn erase(
    conversations: Conversations<'_>,
    id: &ConversationId,
    keys: std::collections::BTreeSet<KeyName>,
    erasure: Erasure,
    at: crate::capability::Datetime,
) -> Result<usize, StoreError> {
    {
        let _held = conversations.lines.locked()?;
        let transcript = read(conversations, id)?;
        let named: std::collections::BTreeSet<&KeyName> = transcript
            .lines()
            .iter()
            .flat_map(|written| written.line.keys())
            .collect();
        if keys.is_empty() || keys.iter().any(|key| !named.contains(key)) {
            return Err(StoreError::not_appended(
                "conversation store",
                format!("an erasure names only words {id} holds"),
            ));
        }
        let held = transcript.lines().len();
        write(
            conversations,
            id,
            held,
            &Entry::Erased { at, keys, erasure },
        )?;
    }
    // The erasure stands whatever the sweep does: a key it could not destroy yet is destroyed by
    // the next sweep (an erasure, a forget, a start), which owes every key an erasure names.
    Ok(sweep(conversations).unwrap_or_else(|failed| {
        tracing::warn!(refused = %failed, "the sweep after an erasure kept keys");
        0
    }))
}

/// The box writes every line; who said one is the line's own `by`.
pub(crate) fn writer() -> Actor {
    Actor::new(crate::secretary::WRITES_AS.actor()).expect("the secretary's actor is a name")
}

/// Open conversation `id` with its first line.
///
/// # Errors
/// [`StoreError::Taken`] for an id already started, and [`push`]'s.
pub fn start(
    conversations: Conversations<'_>,
    id: &ConversationId,
    first: &Entry,
) -> Result<(), StoreError> {
    let _held = conversations.lines.locked()?;
    if keys(conversations)?.contains(id) {
        return Err(StoreError::Taken {
            what: "conversation",
            name: id.to_string(),
        });
    }
    write(conversations, id, 0, first)
}

/// Append `entry` to conversation `id`, its words through the redaction door and locked, resting
/// on every line the conversation holds.
///
/// # Errors
/// [`StoreError::Absent`] for an id never started, an entry the door or the schema refuses, and
/// the stores' own.
pub fn push(
    conversations: Conversations<'_>,
    id: &ConversationId,
    entry: &Entry,
) -> Result<(), StoreError> {
    let _held = conversations.lines.locked()?;
    let held = read(conversations, id)?.lines().len();
    write(conversations, id, held, entry)
}

/// Each key kept first, then the line that names them. A key is never taken back when the
/// append fails: the store may have placed the object before it answered, and a line whose key
/// is gone has lost its words for good, while a key nothing names opens nothing.
fn write(
    conversations: Conversations<'_>,
    id: &ConversationId,
    held: usize,
    entry: &Entry,
) -> Result<(), StoreError> {
    let refused =
        |failed: &dyn std::fmt::Display| StoreError::not_appended("conversation store", failed);
    let held = u32::try_from(held)
        .map_err(|_| refused(&format!("{id} holds more lines than a count can say")))?;
    let (line, keys) = LineEvent::write(id.clone(), writer(), held, entry, conversations.entropy)
        .map_err(|failed| refused(&failed))?;
    for key in &keys {
        conversations.keys.set(&key.name(), key)?;
    }
    conversations.lines.append(line)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::Turn;

    fn id(text: &str) -> ConversationId {
        ConversationId::new(text).expect("a slug")
    }

    fn at(second: u32) -> Datetime {
        Datetime::new(2026, 9, 30, 9, 0, second, 0).expect("an instant")
    }

    fn opened() -> Entry {
        Entry::Opened {
            at: at(0),
            title: crate::secretary::words::Words::kept("Morning".to_owned()),
            model: "a-cli/small".to_owned(),
            kind: crate::secretary::store::Kind::Topic,
        }
    }

    fn person(second: u32, text: &str) -> Entry {
        Entry::Said {
            at: at(second),
            turn: Turn::Person {
                text: text.to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: Some("ada".into()),
        }
    }

    #[test]
    fn lines_read_back_in_the_order_written() {
        let held = crate::secretary::conversations::InMemory::default();
        let store = held.conversations();
        let morning = id("2026-09-30-1");
        start(store, &morning, &opened()).expect("started");
        for (second, text) in [
            (5, "first"),
            (3, "second, on a clock gone back"),
            (3, "ok"),
            (3, "ok"),
        ] {
            push(store, &morning, &person(second, text)).expect("appended");
        }
        let transcript = read(store, &morning).expect("a transcript");
        let entries = transcript.entries();
        assert_eq!(entries.len(), 5, "the same words twice are two lines");
        assert_eq!(entries[1], person(5, "first"));
        assert_eq!(entries[2], person(3, "second, on a clock gone back"));
        let held: Vec<u32> = transcript.lines().iter().map(|w| w.line.held()).collect();
        assert_eq!(held, [0, 1, 2, 3, 4]);
        assert_eq!(transcript.lines()[4].rests_on, [3]);
        assert_eq!(transcript.beneath(4), [0, 1, 2, 3].into());
    }

    #[test]
    fn a_conversation_starts_once_and_is_pushed_to_after() {
        let held = crate::secretary::conversations::InMemory::default();
        let store = held.conversations();
        let morning = id("2026-09-30-1");
        assert!(matches!(
            push(store, &morning, &person(1, "hi")),
            Err(StoreError::Absent { .. })
        ));
        start(store, &morning, &opened()).expect("started");
        assert!(matches!(
            start(store, &morning, &opened()),
            Err(StoreError::Taken { .. })
        ));
        start(store, &id("2026-09-30-2"), &opened()).expect("another");
        assert_eq!(keys(store).expect("keys"), [morning, id("2026-09-30-2")]);
    }

    /// The door is the store's: a credential pushed is gone from every object.
    #[test]
    fn a_pushed_line_holds_no_credential() {
        let held = crate::secretary::conversations::InMemory::default();
        let store = held.conversations();
        let morning = id("2026-09-30-1");
        start(store, &morning, &opened()).expect("started");
        push(
            store,
            &morning,
            &person(1, "reset at https://example.test/r?token=s3cret"),
        )
        .expect("appended");
        for (_, object) in held.lines.objects().expect("objects") {
            let print = prodrome_core::event::canonical_envelope(&object);
            assert!(!print.contains("s3cret"), "{print}");
        }
        assert!(matches!(
            &read(store, &morning).expect("read").entries()[1],
            Entry::Said { turn: Turn::Person { text, .. }, .. }
                if text == "reset at https://example.test/r?token=[redacted]"
        ));
    }

    fn model(second: u32, text: &str) -> Entry {
        Entry::Said {
            at: at(second),
            turn: Turn::Model {
                text: text.to_owned(),
                calls: Vec::new(),
            },
            by: None,
        }
    }

    fn asked() -> Erasure {
        Erasure::Asked {
            by: Actor::new("ada").expect("a name"),
        }
    }

    /// An erasure takes the words it names, reads as erased with the line's shape whole, forgets
    /// what was rendered of them, and destroys their key once: a second sweep finds nothing.
    #[test]
    fn an_erasure_destroys_its_keys_and_their_renders() {
        let held = crate::secretary::conversations::InMemory::default();
        let store = held.conversations();
        let morning = id("2026-09-30-1");
        start(store, &morning, &opened()).expect("started");
        push(store, &morning, &person(1, "the private thing")).expect("said");
        push(store, &morning, &model(2, "First block.\n\nSecond block.")).expect("answered");
        let lines = read(store, &morning).expect("read");
        let answer: std::collections::BTreeSet<KeyName> =
            lines.lines()[2].line.keys().cloned().collect();
        let before = held.keys.names().len();
        assert!(
            answer.iter().all(|key| held.opened.holds(key)),
            "read, so kept"
        );
        let destroyed = erase(store, &morning, answer.clone(), asked(), at(3)).expect("erased");
        assert!(
            answer.iter().all(|key| !held.opened.holds(key)),
            "forgotten with the key"
        );
        assert_eq!(destroyed, 1);
        assert_eq!(held.keys.names().len(), before - 1);
        assert!(answer.iter().all(|key| !held.keys.names().contains(key)));
        assert_eq!(held.forgot.sources(), ["First block.\n\n", "Second block."]);
        let entries = read(store, &morning).expect("read").entries();
        assert_eq!(entries[1], person(1, "the private thing"));
        assert!(matches!(
            &entries[2],
            Entry::Said { turn: Turn::Erased { words, .. }, .. }
                if words.iter().all(|w| matches!(w, Words::Gone { gone: Gone::Erased { .. } }))
        ));
        assert_eq!(sweep(store).expect("swept"), 0);
    }

    /// An erasure names only words of its own conversation, and some.
    #[test]
    fn an_erasure_of_words_elsewhere_is_refused() {
        let held = crate::secretary::conversations::InMemory::default();
        let store = held.conversations();
        let (morning, evening) = (id("2026-09-30-1"), id("2026-09-30-2"));
        start(store, &morning, &opened()).expect("started");
        start(store, &evening, &opened()).expect("started");
        push(store, &evening, &person(1, "elsewhere")).expect("said");
        let theirs: std::collections::BTreeSet<KeyName> =
            read(store, &evening).expect("read").lines()[1]
                .line
                .keys()
                .cloned()
                .collect();
        for keys in [theirs, std::collections::BTreeSet::new()] {
            assert!(matches!(
                erase(store, &morning, keys, asked(), at(2)),
                Err(StoreError::NotAppended { .. })
            ));
        }
        assert_eq!(
            read(store, &evening).expect("read").entries()[1],
            person(1, "elsewhere")
        );
    }

    use proptest::prelude::*;

    /// A cache that cannot forget.
    struct Stuck;

    impl crate::capability::RenderCache for Stuck {
        fn forget(&self, _: &crate::secretary::answer::Renderable) -> Result<(), String> {
            Err("the disk said no".to_owned())
        }
    }

    /// A block that may still be on disk keeps its key, so a later sweep finds it again.
    #[test]
    fn a_render_not_forgotten_keeps_its_key() {
        let held = crate::secretary::conversations::InMemory::default();
        let morning = id("2026-09-30-1");
        start(held.conversations(), &morning, &opened()).expect("started");
        push(held.conversations(), &morning, &model(1, "An answer.")).expect("answered");
        let stuck = Conversations {
            renders: &Stuck,
            ..held.conversations()
        };
        let keys: std::collections::BTreeSet<KeyName> =
            read(stuck, &morning).expect("read").lines()[1]
                .line
                .keys()
                .cloned()
                .collect();
        assert_eq!(erase(stuck, &morning, keys.clone(), asked(), at(2)), Ok(0));
        assert!(matches!(sweep(stuck), Err(StoreError::Unwritable { .. })));
        assert!(keys.iter().all(|key| held.keys.names().contains(key)));
        assert_eq!(sweep(held.conversations()).expect("swept"), 1);
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig { cases: 24, ..Default::default() })]

        /// The lines' laws over any run of turns and erasures: no object holds a turn's words,
        /// after each sweep the keys held are exactly those no erasure names, and what is
        /// destroyed only grows.
        #[test]
        fn the_keys_held_are_the_live_ones_and_the_dead_only_grow(
            steps in proptest::collection::vec(
                proptest::prop_oneof![
                    "[a-z]{6,12}".prop_map(Ok),
                    proptest::collection::vec(0usize..16, 1..3).prop_map(Err),
                ],
                1..16,
            )
        ) {
            let held = crate::secretary::conversations::InMemory::default();
            let store = held.conversations();
            let morning = id("2026-09-30-1");
            start(store, &morning, &opened()).expect("started");
            let mut said = Vec::new();
            let mut dead = std::collections::BTreeSet::new();
            for (second, step) in (1u32..).zip(steps) {
                match step {
                    Ok(words) => {
                        push(store, &morning, &person(second, &format!("<{words}>"))).expect("said");
                        said.push(format!("<{words}>"));
                    }
                    Err(lines) => {
                        let read = read(store, &morning).expect("read");
                        let keys: std::collections::BTreeSet<KeyName> = lines
                            .iter()
                            .filter_map(|&line| read.lines().get(line))
                            .flat_map(|written| written.line.keys().cloned())
                            .collect();
                        if !keys.is_empty() {
                            erase(store, &morning, keys, asked(), at(second)).expect("erased");
                        }
                    }
                }
                let objects = held.lines.objects().expect("objects");
                for (_, object) in &objects {
                    let print = prodrome_core::event::canonical_envelope(object);
                    for words in &said {
                        prop_assert!(!print.contains(words.as_str()));
                    }
                }
                let erased = erased(&objects);
                let named: std::collections::BTreeSet<KeyName> = objects
                    .iter()
                    .filter_map(|(_, object)| object.event())
                    .flat_map(|line| line.keys().cloned())
                    .collect();
                let live: std::collections::BTreeSet<KeyName> = named
                    .iter()
                    .filter(|key| !erased.contains_key(*key))
                    .cloned()
                    .collect();
                let kept: std::collections::BTreeSet<KeyName> =
                    held.keys.names().into_iter().collect();
                prop_assert_eq!(&kept, &live);
                let now: std::collections::BTreeSet<KeyName> =
                    named.difference(&kept).cloned().collect();
                prop_assert!(dead.is_subset(&now));
                dead = now;
            }
        }
    }
}
