//! The conversations as one prodrome at the [`LineEvent`] schema, keyed by conversation, each line
//! resting on the lines its writer held. A slot whose key an erasure names reads as erased, and one
//! whose key is not held reads as missing.
//!
//! [`Transcript`] is one conversation's lines in that order, the box's lines its spine
//! ([`sequence::sequenced`]). [`Transcript::stored`], [`Transcript::owed`] and [`Stored::lines`]
//! are reads of it.
//!
//! Every line is written through [`push`] or [`start`], which take its words through the
//! redaction door and lock each slot under a fresh key before the store sees it: nothing enters
//! the lines with its words in the clear.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::{Actor, Envelope, Hash};
use prodrome_core::literal::Datetime;
use prodrome_core::registers::Node;
use prodrome_core::schema::Schema as _;

pub use crate::secretary::claim::Retry;
use crate::secretary::claim::{Asked, Bound, Claims, Due, Line, Mark, Standing, Wakes};
use crate::secretary::conversations::Conversations;
use crate::secretary::held_lines::HeldLines;
use crate::secretary::line_event::LineEvent;
use crate::secretary::line_shape::LineShape;
use crate::secretary::sequence;
use crate::wire::lines::LineWriter;
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::SessionId;
use suzatary_conversations::secretary::store::{
    Covered, Entry, NotAConversation, RunId, Said, Stored,
};
use suzatary_conversations::secretary::turn_shape::TurnShape;
use suzatary_memory::secretary::erasure::Erasure;
use suzatary_memory::secretary::holder::Holder;
use suzatary_memory::secretary::line_words::LineWords;
use suzatary_memory::secretary::locked_text::{KeyName, TextKey};
use suzatary_memory::secretary::slots::Slots;
use suzatary_memory::secretary::words::{Gone, Words};
use suzatary_store::store::{Prodrome, StoreError};

/// One conversation's lines in causal order, their fold, and the claims' reading of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    lines: Vec<Written>,
    stored: std::sync::Arc<Stored>,
    /// Each turn's line, by the turn's index in [`Stored::turns`].
    turns: Vec<usize>,
    /// What each line is to the claim, by its position.
    marks: Vec<Mark<RunId>>,
    claims: Claims<RunId>,
}

/// One line beside its name, its reading and the lines it rests on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
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
    pub name: &'n Hash,
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
        let lines = || writes.iter().map(|write| (write.name, write.line));
        let erased = erasures(lines());
        let superseded = superseded(lines(), slots);
        let reseals = reseals(lines());
        let read = Through {
            erased: &erased,
            superseded: &superseded,
            reseals: &reseals,
            slots,
        };
        let read: Vec<_> = writes
            .iter()
            .map(|write| (*write, write.line.entry(&read)))
            .collect();
        Self::read(id, &read)
    }

    /// One conversation's writes, in any order, each beside its entry as read, put in causal
    /// order and folded.
    ///
    /// # Errors
    /// [`NotAConversation`] for lines the fold refuses.
    pub(crate) fn read(
        id: &ConversationId,
        writes: &[(Write<'_>, Entry)],
    ) -> Result<Self, NotAConversation> {
        let order = sequenced(
            &writes
                .iter()
                .map(|(write, _)| sequence::Node {
                    name: write.name,
                    at: write.line.at(),
                    parents: write.parents,
                    writer: LineWriter::of(write.line.actor()),
                })
                .collect::<Vec<_>>(),
        );
        let lines: Vec<Written> = order
            .into_iter()
            .map(|(at, rests_on)| Written {
                name: writes[at].0.name.clone(),
                line: writes[at].0.line.clone(),
                entry: writes[at].1.clone(),
                rests_on,
            })
            .collect();
        let stored = Stored::of_lines(id, lines.iter().map(|w| &w.entry))?;
        // The fold makes a turn of each said line and each stop, and of nothing else.
        let turns: Vec<usize> = lines
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, w)| matches!(w.entry, Entry::Said { .. } | Entry::Stopped { .. }))
            .map(|(at, _)| at)
            .collect();
        let marks = marks(&lines, &stored, &turns);
        let claims = Claims::of(
            &marks
                .iter()
                .zip(&lines)
                .map(|(mark, line)| Line {
                    mark,
                    rests_on: &line.rests_on,
                })
                .collect::<Vec<_>>(),
        );
        Ok(Self {
            lines,
            stored: std::sync::Arc::new(stored),
            turns,
            marks,
            claims,
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
        std::sync::Arc::unwrap_or_clone(self.stored)
    }

    /// The transcript register, shared.
    #[must_use]
    pub(crate) fn shared(&self) -> &std::sync::Arc<Stored> {
        &self.stored
    }

    /// The owed register: every live turn the claims do not read as settled, with its index. A
    /// person's or an act's turn is settled by a closed round that covered it, or by a stop or a
    /// close with no round open that rests on it; a model's or a verb's by any close resting on it.
    /// It reads descent and claims, not position, so it is the same for every order a replica
    /// received the lines in.
    #[must_use]
    pub fn owed(&self) -> Vec<(usize, &Said)> {
        self.stored
            .live()
            .into_iter()
            .filter(|(turn, _)| self.claims.standing(self.turns[*turn]) != Standing::Settled)
            .collect()
    }

    /// Whether a turn that wakes a model is owed: what the conversation's answer rule reads.
    #[must_use]
    pub fn owes_a_round(&self) -> bool {
        self.owed()
            .iter()
            .any(|(_, said)| suzatary_conversations::secretary::queue::wakes(said))
    }

    /// The claims' reading of these lines.
    #[must_use]
    pub(crate) fn claims(&self) -> &Claims<RunId> {
        &self.claims
    }

    /// Every live request (a person's or an act's turn, or a model's or a verb's) as a claim step
    /// at `now` reads it.
    #[must_use]
    pub(crate) fn asked(&self, now: Datetime) -> Vec<Asked> {
        let now = prodrome_core::fpl::instant_of(now);
        self.stored
            .live()
            .into_iter()
            .map(|(turn, said)| (self.turns[turn], said))
            .filter(|(line, _)| matches!(self.marks[*line], Mark::Request | Mark::Answer))
            .map(|(at, said)| Asked {
                at,
                wakes: if suzatary_conversations::secretary::queue::wakes(said) {
                    Wakes::Model
                } else {
                    Wakes::RidesAlong
                },
                bound: if now - prodrome_core::fpl::instant_of(said.at) <= ANSWERED_WITHIN {
                    Bound::Within
                } else {
                    Bound::Past
                },
            })
            .collect()
    }

    /// What the open round of `run` sends its model: every live turn it covers, in order.
    #[must_use]
    pub fn sending(&self, run: &RunId) -> Vec<(usize, &Said)> {
        let covered: &[usize] = self
            .claims
            .open(run)
            .map_or(&[], |round| &self.claims.rounds()[round].covers);
        self.stored
            .live()
            .into_iter()
            .filter(|(turn, _)| covered.contains(&self.turns[*turn]))
            .collect()
    }
}

/// What a round's write for one question did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answering {
    /// The lines were written.
    Written,
    /// The round was closed already (a stop, or the box's close of a run it read as gone), so
    /// nothing was written.
    Answered,
}

/// [`push`] each of `entries`, a round's lines, in one hold of the lock, while the round of `run`
/// is open: a round that another line closed writes nothing more.
///
/// # Errors
/// As [`push`].
pub fn push_open(
    conversations: Conversations<'_>,
    id: &ConversationId,
    run: &RunId,
    entries: &[Entry],
) -> Result<Answering, StoreError> {
    let _held = conversations.lines.locked()?;
    let read = read(conversations, id)?;
    if read.claims().open(run).is_none() {
        return Ok(Answering::Answered);
    }
    for (more, entry) in entries.iter().enumerate() {
        write(conversations, id, read.lines().len() + more, entry)?;
    }
    Ok(Answering::Written)
}

/// How long after a request is made a round may still claim it: past it, the box closes it
/// unanswered, and asking again is the person's.
pub const ANSWERED_WITHIN: chrono::Duration = chrono::Duration::hours(1);

/// What a claim step did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claimed {
    /// Nothing was owed, or a round of another run is running.
    Nothing,
    /// A `Began` of the run was written: the round may ask.
    Began,
    /// The box's close was written: of a lost round it will not retry, or of a question past
    /// [`ANSWERED_WITHIN`].
    Closed,
}

/// The claim step of conversation `id` at `now`, under the lines' lock: what the claim reading
/// owes (a first claim, a retry of a lost round, the box's close, or nothing while another run's
/// round runs), written for the round of `run`. A covered turn's taint is the box's judgment of it now. The
/// box's close is `close` of the conversation as read.
///
/// # Errors
/// As [`push`].
pub fn claim(
    conversations: Conversations<'_>,
    id: &ConversationId,
    run: &RunId,
    now: Datetime,
    retry: Retry,
    close: impl FnOnce(&Stored) -> Vec<Entry>,
) -> Result<Claimed, StoreError> {
    let _held = conversations.lines.locked()?;
    let read = read(conversations, id)?;
    let held = read.lines().len();
    let covers =
        match read
            .claims()
            .due(&read.asked(now), |run| conversations.runs.gone(run), retry)
        {
            Due::Nothing => return Ok(Claimed::Nothing),
            Due::Claim(covers) | Due::Retry(covers) => covers,
            Due::Close => {
                for (more, entry) in close(read.stored()).iter().enumerate() {
                    write(conversations, id, held + more, entry)?;
                }
                return Ok(Claimed::Closed);
            }
        };
    write(conversations, id, held, &read.began(&covers, run, now))?;
    Ok(Claimed::Began)
}

/// Before the round of `run` asks again, under the lines' lock: a `Began` of its run covering
/// every request queued, when there is any. A retry round takes none.
///
/// # Errors
/// As [`push`].
pub fn join(
    conversations: Conversations<'_>,
    id: &ConversationId,
    run: &RunId,
    now: Datetime,
) -> Result<(), StoreError> {
    let _held = conversations.lines.locked()?;
    let read = read(conversations, id)?;
    let covers = read.claims().joins(&read.asked(now), run);
    if covers.is_empty() {
        return Ok(());
    }
    write(
        conversations,
        id,
        read.lines().len(),
        &read.began(&covers, run, now),
    )
}

impl Transcript {
    /// The lines the claim step of a start at `now` claims: a retry of a lost round, or a first
    /// claim that covers a person's question. A conversation owed only an act, a note or the
    /// box's close waits for its next write, as a start before claims left it.
    #[must_use]
    pub(crate) fn resumes(
        &self,
        now: Datetime,
        gone: impl Fn(&RunId) -> bool,
        retry: Retry,
    ) -> Vec<usize> {
        match self.claims.due(&self.asked(now), gone, retry) {
            Due::Retry(covers) => covers,
            Due::Claim(covers)
                if covers.iter().any(|&line| {
                    matches!(
                        self.lines[line].line.shape(),
                        LineShape::Said {
                            turn: TurnShape::Person { .. },
                            ..
                        }
                    )
                }) =>
            {
                covers
            }
            Due::Claim(_) | Due::Close | Due::Nothing => Vec::new(),
        }
    }

    /// The claim of `run` over the lines at `covers`, each judged as the box reads it now.
    fn began(&self, covers: &[usize], run: &RunId, now: Datetime) -> Entry {
        let turn_of = |line: usize| self.turns.iter().position(|&at| at == line);
        Entry::Began {
            at: now,
            covers: covers
                .iter()
                .filter_map(|&line| {
                    Some(Covered {
                        line: self.lines[line].name.clone(),
                        taint: suzatary_conversations::secretary::taint::turn(
                            &self.stored.turns[turn_of(line)?],
                        ),
                    })
                })
                .collect(),
            run: run.clone(),
        }
    }
}

/// Every conversation a start at `now` resumes, oldest id first: one whose claim step would retry
/// a lost round, or claim a person's question. One owed only an act, a note or the box's close
/// waits for its next write, as a start before claims left it.
///
/// # Errors
/// The store's.
pub fn resumed(
    conversations: Conversations<'_>,
    now: Datetime,
    retry: Retry,
) -> Result<Vec<ConversationId>, StoreError> {
    let mut resumed: Vec<ConversationId> = each(conversations)?
        .into_iter()
        .filter(|(_, read)| {
            read.as_ref().is_ok_and(|read| {
                !read
                    .resumes(now, |run| conversations.runs.gone(run), retry)
                    .is_empty()
            })
        })
        .map(|(id, _)| id)
        .collect();
    resumed.reverse();
    Ok(resumed)
}

/// What each line is to the claim: a live answer or stop closes, and one a rewind set aside is
/// withdrawn; a `Began` claims, its covers by their positions here.
fn marks(lines: &[Written], stored: &Stored, turns: &[usize]) -> Vec<Mark<RunId>> {
    let mut turn_of = vec![None; lines.len()];
    for (turn, &line) in turns.iter().enumerate() {
        turn_of[line] = Some(turn);
    }
    let position: BTreeMap<&Hash, usize> = lines
        .iter()
        .enumerate()
        .map(|(at, written)| (&written.name, at))
        .collect();
    let live = |at: usize| turn_of[at].is_some_and(|turn| stored.is_live(turn));
    lines
        .iter()
        .enumerate()
        .map(|(at, written)| match written.line.shape() {
            LineShape::Said { turn, .. } => match turn {
                TurnShape::Person { .. } | TurnShape::Act { .. } => Mark::Request,
                TurnShape::Model { calls } if calls.is_empty() && live(at) => Mark::Close,
                TurnShape::Model { calls } if calls.is_empty() => Mark::Withdrawn,
                TurnShape::Model { .. } | TurnShape::Tool { .. } => Mark::Answer,
            },
            LineShape::Stopped { .. } if live(at) => Mark::Stop,
            LineShape::Stopped { .. } => Mark::Withdrawn,
            LineShape::Began { covers, run } => Mark::Claim {
                run: run.clone(),
                covers: covers
                    .iter()
                    .filter_map(|covered| position.get(&covered.line).copied())
                    .collect(),
            },
            LineShape::Opened { .. }
            | LineShape::SetUp { .. }
            | LineShape::Titled { .. }
            | LineShape::Proposed { .. }
            | LineShape::Reviewed { .. }
            | LineShape::ReviewFailed { .. }
            | LineShape::Rewound { .. }
            | LineShape::MemorySeen { .. }
            | LineShape::Session { .. }
            | LineShape::Unheard { .. }
            | LineShape::Kept { .. }
            | LineShape::Scoped { .. }
            | LineShape::Coding { .. }
            | LineShape::Erased { .. }
            | LineShape::Resealed { .. } => Mark::Other,
        })
        .collect()
}

/// A conversation's reading is named by its heads, the lines no other line rests on: the down-set
/// they close is every line, so two replicas holding the same lines name one reading.
impl suzatary_triage::rule::Named for Transcript {
    fn name(&self) -> suzatary_triage::rule::ReadingName {
        let rested_on: std::collections::BTreeSet<usize> = self
            .lines
            .iter()
            .flat_map(|written| written.rests_on.iter().copied())
            .collect();
        suzatary_triage::rule::ReadingName::of(
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

/// [`erasures`], each beside the name of the erasure line that says why. A slot erased is
/// erased in every seal of it: the key of each reseal of a slot an erasure names is named too.
#[must_use]
pub(crate) fn erasers<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
) -> BTreeMap<KeyName, (&'l Hash, &'l Erasure)> {
    let lines: Vec<(&Hash, &LineEvent)> = lines.into_iter().collect();
    let mut named: BTreeMap<KeyName, (&Hash, &Erasure)> = BTreeMap::new();
    for (name, line) in &lines {
        if let LineShape::Erased { keys, erasure } = line.shape() {
            for key in keys {
                let at = named.entry(key.clone()).or_insert((name, erasure));
                if *name < at.0 {
                    *at = (name, erasure);
                }
            }
        }
    }
    for (_, line) in &lines {
        if let LineShape::Resealed { of, .. } = line.shape()
            && let Some(by) = named.get(of).copied()
        {
            named.extend(line.keys().map(|key| (key.clone(), by)));
        }
    }
    named
}

/// Every key a reseal among `lines` supersedes here: the key of each seal a slot was sealed again
/// from, where `slots` opens that reseal's one slot (reads it as anything but missing), or where
/// a later reseal supersedes the reseal's own key, since that reseal holds the same words. Its
/// words then stand in a reseal, so the key is destroyed like an erased one, and the slot reads
/// from the reseal. A reseal whose key is not held, and that nothing later supersedes, supersedes
/// nothing, so a store restored with its lines and without their keys keeps reading the old seal
/// until a run seals it again. Monotone in the lines and in the keys held.
#[must_use]
pub fn superseded<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
    slots: &impl Slots,
) -> BTreeSet<KeyName> {
    superseded_among(lines, &|_| true, slots)
}

/// The keys of [`superseded`] that `asked` admits; no other key opens a reseal.
pub fn superseded_among<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
    asked: &dyn Fn(&KeyName) -> bool,
    slots: &impl Slots,
) -> BTreeSet<KeyName> {
    let resealed = resealed_from(lines);
    let from = |key: &KeyName| -> Vec<&'l LineEvent> {
        resealed
            .get(key)
            .into_iter()
            .flatten()
            .map(|(_, line)| *line)
            .collect()
    };
    resealed
        .keys()
        .filter(|key| asked(key) && supersedes(key, from, slots))
        .map(|key| (*key).clone())
        .collect()
}

/// Every reseal among `lines`, beside its name, by the key of the seal it was sealed again from.
fn resealed_from<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
) -> BTreeMap<&'l KeyName, Vec<(&'l Hash, &'l LineEvent)>> {
    let mut by: BTreeMap<&KeyName, Vec<(&Hash, &LineEvent)>> = BTreeMap::new();
    for (name, line) in lines {
        if let LineShape::Resealed { from, .. } = line.shape() {
            by.entry(from).or_default().push((name, line));
        }
    }
    by
}

/// Whether a reseal supersedes the seal under `key`: one sealed again from it whose slot `slots`
/// opens, or whose own key a later reseal supersedes in turn, walking `resealed_from`, the
/// reseals sealed again from each key. Each key is walked once, so the walk ends whatever the
/// lines name.
pub fn supersedes<'l>(
    key: &KeyName,
    resealed_from: impl Fn(&KeyName) -> Vec<&'l LineEvent>,
    slots: &impl Slots,
) -> bool {
    let mut walked = BTreeSet::new();
    let mut walking = vec![key.clone()];
    while let Some(key) = walking.pop() {
        if !walked.insert(key.clone()) {
            continue;
        }
        for reseal in resealed_from(&key) {
            if reseal.slot_opens(0, slots) {
                return true;
            }
            walking.extend(reseal.keys().cloned());
        }
    }
    false
}

/// What a store's lines retire of their keys, a function of the lines alone, so it is read once
/// per set of heads: every key an erasure of its own conversation names ([`erased`]), and each
/// conversation's reseals by the key of the seal each was sealed from, so
/// whether a key is superseded is asked of its own conversation's reseals descending from it
/// alone ([`supersedes`]), as [`dead`] asks it.
#[derive(Debug, Default)]
pub struct Retired {
    /// The keys erased, with why.
    pub erased: BTreeMap<KeyName, Erasure>,
    resealed: BTreeMap<ConversationId, BTreeMap<KeyName, Vec<Hash>>>,
}

impl Retired {
    /// What `objects` retire.
    #[must_use]
    pub fn of<'l>(objects: impl IntoIterator<Item = impl Named<'l>>) -> Self {
        let lines: Vec<(&Hash, &LineEvent)> = lines_of(objects).collect();
        let resealed = by_conversation(lines.iter().copied())
            .into_iter()
            .map(|(id, lines)| {
                let index = resealed_from(lines)
                    .into_iter()
                    .map(|(from, reseals)| {
                        (
                            from.clone(),
                            reseals.into_iter().map(|(name, _)| name.clone()).collect(),
                        )
                    })
                    .collect();
                (id.clone(), index)
            })
            .collect();
        Self {
            erased: erased_lines(lines),
            resealed,
        }
    }

    /// Whether `key`, a key of a line of conversation `id`, is dead: erased, or superseded by a
    /// reseal of that conversation that `slots` opens, each reseal read by its name through `line`.
    pub fn retires<'l>(
        &self,
        id: &ConversationId,
        key: &KeyName,
        line: impl Fn(&Hash) -> Option<&'l LineEvent>,
        slots: &impl Slots,
    ) -> bool {
        let resealed = self.resealed.get(id);
        self.erased.contains_key(key)
            || supersedes(
                key,
                |from| {
                    resealed
                        .and_then(|index| index.get(from))
                        .into_iter()
                        .flatten()
                        .filter_map(&line)
                        .collect()
                },
                slots,
            )
    }
}

/// Every reseal among `lines`, by the key of the slot it seals, those under the current rules
/// first, and among those under the same rules the newest first: the one sealed again from
/// another's key comes before that other, however far down the chain. Where a slot's own seal
/// does not open, it is read from the first of these that does, so a slot whose every reseal is
/// under older rules reads its newest, which the current rules scrub least.
#[must_use]
pub fn reseals<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
) -> BTreeMap<KeyName, Vec<&'l LineEvent>> {
    let mut by: BTreeMap<KeyName, Vec<(&Hash, &LineEvent)>> = BTreeMap::new();
    for (name, line) in lines {
        if let LineShape::Resealed { of, .. } = line.shape() {
            by.entry(of.clone()).or_default().push((name, line));
        }
    }
    by.into_iter()
        .map(|(of, seals)| {
            let mut ranked: Vec<(bool, std::cmp::Reverse<usize>, &Hash, &LineEvent)> = seals
                .iter()
                .map(|(name, line)| {
                    let stale = !line.scrubbed_under().all(|under| under.is_current(()));
                    (
                        stale,
                        std::cmp::Reverse(depth(line, &of, &seals)),
                        *name,
                        *line,
                    )
                })
                .collect();
            ranked.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
            (of, ranked.into_iter().map(|(.., line)| line).collect())
        })
        .collect()
}

/// How many reseals down from the slot's first seal `reseal` stands: one when it was sealed
/// again from the first seal `of`, one more than the reseal whose own key it was sealed from
/// otherwise. A chain is walked at most once through `seals`, so the count ends whatever the
/// lines name.
fn depth(reseal: &LineEvent, of: &KeyName, seals: &[(&Hash, &LineEvent)]) -> usize {
    let mut depth = 1;
    let mut at = reseal;
    while depth <= seals.len() {
        let LineShape::Resealed { from, .. } = at.shape() else {
            break;
        };
        if from == of {
            break;
        }
        match seals
            .iter()
            .find(|(_, other)| other.keys().next() == Some(from))
        {
            Some((_, below)) => {
                at = below;
                depth += 1;
            }
            None => break,
        }
    }
    depth
}

/// A slot as one conversation reads it: erased where `erased` names its key, before anything
/// kept or held is asked; from the first of the slot's `reseals` that opens where `superseded`
/// names its key, so its own seal is never opened and scrubbed again; else as `slots` reads it,
/// and where that is missing, from the first reseal that opens.
pub struct Through<'a, S> {
    /// The keys erased, with why.
    pub erased: &'a BTreeMap<KeyName, Erasure>,
    /// The keys a reseal that opens here supersedes ([`superseded`]).
    pub superseded: &'a BTreeSet<KeyName>,
    /// The reseals of each slot ([`reseals`]).
    pub reseals: &'a BTreeMap<KeyName, Vec<&'a LineEvent>>,
    /// How a slot is read by its key.
    pub slots: &'a S,
}

impl<S: Slots> Slots for Through<'_, S> {
    fn words(
        &self,
        name: &KeyName,
        open: &dyn Fn(&TextKey) -> Option<Words<LineWords>>,
    ) -> Words<LineWords> {
        if let Some(erasure) = self.erased.get(name) {
            return Gone::Erased {
                erasure: erasure.clone(),
            }
            .into();
        }
        let resealed = || {
            self.reseals
                .get(name)
                .into_iter()
                .flatten()
                .map(|reseal| reseal.slot_words(0, self))
                .find(|words| !missing(words))
                .unwrap_or_else(|| Gone::Missing.into())
        };
        if self.superseded.contains(name) {
            return resealed();
        }
        match self.slots.words(name, open) {
            Words::Gone {
                gone: Gone::Missing,
            } => resealed(),
            read => read,
        }
    }
}

fn missing(words: &Words<LineWords>) -> bool {
    matches!(
        words,
        Words::Gone {
            gone: Gone::Missing
        }
    )
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

/// An object of the lines beside its name, as a slice of a store's objects yields it and as a
/// store's graph does.
pub trait Named<'l> {
    /// The name and the object.
    fn named(self) -> (&'l Hash, &'l Envelope<LineEvent>);
}

impl<'l> Named<'l> for &'l (Hash, Envelope<LineEvent>) {
    fn named(self) -> (&'l Hash, &'l Envelope<LineEvent>) {
        (&self.0, &self.1)
    }
}

impl<'l> Named<'l> for (&'l Hash, &'l Envelope<LineEvent>) {
    fn named(self) -> (&'l Hash, &'l Envelope<LineEvent>) {
        self
    }
}

impl<'l> Named<'l> for &'l (&'l Hash, &'l Envelope<LineEvent>) {
    fn named(self) -> (&'l Hash, &'l Envelope<LineEvent>) {
        *self
    }
}

/// The objects' lines, by name.
fn lines_of<'l>(
    objects: impl IntoIterator<Item = impl Named<'l>>,
) -> impl Iterator<Item = (&'l Hash, &'l LineEvent)> {
    objects.into_iter().filter_map(|object| {
        let (name, object) = object.named();
        object.event().map(|line| (name, line))
    })
}

/// Every slot whose key an erasure line of its own conversation names, or a reseal of it that
/// `slots` opens supersedes ([`superseded`]), as the line, the slot and its key's name: what a
/// sweep destroys, on the box or in a tab. Monotone in the lines and the keys held, since a key an
/// erasure names or a reseal supersedes stays so as lines and keys are added.
#[must_use]
pub fn dead<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
    slots: &impl Slots,
) -> Vec<(&'l LineEvent, usize, &'l KeyName)> {
    dead_among(lines, &|_| true, slots)
}

/// The slots of [`dead`] whose key `asked` admits; no other key opens a reseal.
pub fn dead_among<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
    asked: &dyn Fn(&KeyName) -> bool,
    slots: &impl Slots,
) -> Vec<(&'l LineEvent, usize, &'l KeyName)> {
    by_conversation(lines)
        .into_values()
        .flat_map(|lines| {
            let erased = erasures(lines.iter().copied());
            let superseded = superseded_among(lines.iter().copied(), asked, slots);
            lines
                .into_iter()
                .flat_map(|(_, line)| {
                    line.keys()
                        .enumerate()
                        .map(move |(slot, key)| (line, slot, key))
                })
                .filter(|(_, _, key)| {
                    asked(key) && (erased.contains_key(*key) || superseded.contains(*key))
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The answer blocks slot `slot`'s words were rendered to, opened by `key`: what a replica
/// forgets before it destroys the key, since nothing finds them once the words are gone. A slot
/// under older rules was rendered from its words as they were locked while those rules stood, and
/// from its words as the current rules read them after, so the blocks of both are named; the
/// words as locked only name blocks, and are shown to no one.
#[must_use]
pub fn fed(line: &LineEvent, slot: usize, key: &TextKey) -> Vec<suzatary_base::answer::Renderable> {
    let mut fed: Vec<suzatary_base::answer::Renderable> = Vec::new();
    for words in [line.unscrubbed(slot, key), line.open(slot, key)] {
        if let Some(LineWords::Model { text }) = words {
            for block in suzatary_base::answer::blocks(&text) {
                let block = suzatary_base::answer::Renderable::of(&block.source);
                if !fed.contains(&block) {
                    fed.push(block);
                }
            }
        }
    }
    fed
}

/// Every key the objects' lines name that an erasure of the same conversation names: the keys
/// whose words are erased, each with why. A key an erasure names in another conversation is no
/// key of its, and erases nothing.
#[must_use]
pub fn erased<'l>(objects: impl IntoIterator<Item = impl Named<'l>>) -> BTreeMap<KeyName, Erasure> {
    erased_lines(lines_of(objects))
}

/// [`erased`] of the lines themselves.
pub fn erased_lines<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
) -> BTreeMap<KeyName, Erasure> {
    by_conversation(lines)
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

/// One conversation's lines in the order every reading takes ([`sequence::sequenced`]), the box's
/// lines its spine: each with the positions in that order of the lines it rests on. A parent that
/// is no line here (the genesis) orders nothing.
#[must_use]
pub fn sequenced(lines: &[sequence::Node<'_, Hash, Datetime, LineWriter>]) -> sequence::Order {
    sequence::sequenced(lines, LineWriter::spine)
}

/// Whether `actor` is the box's: the writer of the spine of every conversation's order.
#[must_use]
pub fn by_the_box(actor: &Actor) -> bool {
    actor.as_str() == suzatary_base::writer::Writer::Assistant.actor()
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

fn unreadable(failed: impl std::fmt::Display) -> StoreError {
    StoreError::unreadable("conversation store", failed)
}

/// One conversation's reading, or why its lines are none.
pub type Read = Result<std::sync::Arc<Transcript>, NotAConversation>;

/// Every conversation, newest id first, each read on its own, so one whose lines the fold refuses
/// is that conversation's refusal and no other's: one fold of the store for all of them.
///
/// # Errors
/// The store's own.
pub fn each(conversations: Conversations<'_>) -> Result<Vec<(ConversationId, Read)>, StoreError> {
    conversations.reads().fold(HeldLines::transcripts)
}

/// What `verify` reports of the conversations, read against `inbox` and by their shapes alone,
/// since whether a conversation folds never rests on its words: the store's own findings,
/// every conversation whose lines do not fold, and every proposal line naming a record the inbox
/// does not hold, by its reference. An inbox that does not fold, and a line answered and never
/// asked, are the inbox's own findings ([`suzatary_inbox::secretary::inbox::findings`]), so each is named
/// once.
#[must_use]
pub fn findings(
    lines: &dyn Prodrome<LineEvent>,
    inbox: &dyn Prodrome<suzatary_inbox::secretary::inbox_event::InboxEvent>,
) -> Vec<String> {
    let mut found = lines.verify();
    let inbox = suzatary_inbox::secretary::inbox::fold(inbox).ok();
    let nodes = match lines.nodes() {
        Ok(nodes) => nodes,
        Err(failed) => {
            found.push(failed.to_string());
            return found;
        }
    };
    let keyless = |_: &KeyName| -> Words<TextKey> { Gone::Missing.into() };
    for (id, writes) in writes(&nodes).into_iter().rev() {
        let stored = match Transcript::of(id, &writes, &keyless) {
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
                .filter(|unplaced| {
                    unplaced.why == suzatary_conversations::secretary::store::Unheld::NotHeld
                })
                .map(|unplaced| {
                    let line = suzatary_base::reference::Reference::Proposal(
                        suzatary_base::reference::ProposalRef {
                            conversation: stored.id.clone(),
                            index: unplaced.asking.index,
                        },
                    );
                    format!("{line} has no card: {}", unplaced.why)
                }),
        );
    }
    found
}

/// One conversation, read.
///
/// # Errors
/// [`StoreError::Absent`] for an id never started, and [`each`]'s.
pub fn read(
    conversations: Conversations<'_>,
    id: &ConversationId,
) -> Result<std::sync::Arc<Transcript>, StoreError> {
    conversations
        .reads()
        .fold(|lines| lines.transcript(id))?
        .ok_or_else(|| StoreError::Absent {
            what: "conversation",
            name: id.to_string(),
        })?
        .map_err(unreadable)
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

/// Destroy every key an erasure line names that the key store still holds, each once every
/// holder of its words is emptied ([`Holder::ALL`]), and say how many. The sweep is monotone: it
/// takes no hold, and running it twice, or on two replicas, destroys what running it once does.
///
/// # Errors
/// The stores' own, and [`StoreError::Unwritable`] naming every key kept because a copy of its
/// words may still be on disk; the next sweep tries again.
pub fn sweep(conversations: Conversations<'_>) -> Result<usize, StoreError> {
    let graph = conversations.lines.graph()?;
    // A key neither holds has nothing left to empty; an unreadable store is asked, and refuses.
    let held = |name: &KeyName| {
        conversations.opened.holds(name) || !matches!(conversations.keys.get(name), Ok(None))
    };
    destroy(
        conversations,
        &sessions(lines_of(graph.objects())),
        dead_among(lines_of(graph.objects()), &held, &|name: &KeyName| {
            conversations.reads().key(name)
        }),
    )
}

/// Each conversation's model sessions, with the directory each was opened in.
fn sessions<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
) -> BTreeMap<&'l ConversationId, BTreeSet<(&'l SessionId, &'l str)>> {
    let mut sessions: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
    for (_, line) in lines {
        if let LineShape::Session { session, cwd, .. } | LineShape::Kept { session, cwd, .. } =
            line.shape()
        {
            sessions
                .entry(line.conversation())
                .or_default()
                .insert((session, cwd.as_str()));
        }
    }
    sessions
}

/// [`sweep`] of `slots`.
fn destroy<'l>(
    conversations: Conversations<'_>,
    sessions: &BTreeMap<&ConversationId, BTreeSet<(&SessionId, &str)>>,
    slots: impl IntoIterator<Item = (&'l LineEvent, usize, &'l KeyName)>,
) -> Result<usize, StoreError> {
    let mut destroyed = 0;
    let mut kept = Vec::new();
    for (line, slot, name) in slots {
        let Some(key) = conversations.keys.get(name)? else {
            conversations.opened.forget(name);
            continue;
        };
        // A copy left on disk keeps the key, so the next sweep can find the copy again.
        let emptied = Holder::ALL.into_iter().try_for_each(|holder| match holder {
            Holder::Opened => {
                conversations.opened.forget(name);
                Ok(())
            }
            Holder::Renders => fed(line, slot, &key)
                .iter()
                .try_for_each(|block| conversations.renders.forget(block)),
            Holder::ModelSessions => sessions
                .get(line.conversation())
                .into_iter()
                .flatten()
                .try_for_each(|(id, cwd)| conversations.sessions.forget(id, cwd)),
            Holder::Keys => conversations
                .keys
                .remove(name)
                .map_err(|failed| failed.to_string()),
            // Out of the box's reach: the erase question names them.
            Holder::Vault | Holder::DataHistory | Holder::DeviceLines => Ok(()),
        });
        match emptied {
            Ok(()) => destroyed += 1,
            Err(failed) => kept.push(format!("{name}: {failed}")),
        }
    }
    if kept.is_empty() {
        Ok(destroyed)
    } else {
        Err(StoreError::unwritable(
            "a copy of the words",
            format!(
                "kept {} key(s) whose words may still be on disk: {}",
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
/// [`StoreError::NotAppended`] for no keys, or a key that is not the first key of a slot of `id`:
/// an erasure erases only its own conversation's words, and names a slot sealed again by the key
/// it was first sealed under, which erases every seal of it. And [`push`]'s.
pub fn erase(
    conversations: Conversations<'_>,
    id: &ConversationId,
    keys: std::collections::BTreeSet<KeyName>,
    erasure: Erasure,
    at: suzatary_base::capability::Datetime,
) -> Result<usize, StoreError> {
    {
        let _held = conversations.lines.locked()?;
        let transcript = read(conversations, id)?;
        // A slot is named by the key it was first sealed under. An erasure of a reseal's own key
        // alone would leave the slot's first seal open on a device that never held that key.
        let slots: std::collections::BTreeSet<&KeyName> = transcript
            .lines()
            .iter()
            .filter(|written| !matches!(written.line.shape(), LineShape::Resealed { .. }))
            .flat_map(|written| written.line.keys())
            .collect();
        if keys.is_empty() || keys.iter().any(|key| !slots.contains(key)) {
            return Err(StoreError::not_appended(
                "conversation store",
                format!("an erasure names only slots {id} holds, each by its first key"),
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
#[must_use]
pub fn writer() -> Actor {
    Actor::new(suzatary_base::writer::Writer::Assistant.actor())
        .expect("the secretary's actor is a name")
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

/// Begin a new conversation with `first`, its id a function of that line
/// ([`LineEvent::begun_id`]), as a tab's opening line is; answers the id.
///
/// # Errors
/// [`StoreError::Taken`] where the store holds a line of that id already, an entry the door or
/// the schema refuses, and the stores' own.
pub fn begin(
    conversations: Conversations<'_>,
    first: &Entry,
) -> Result<ConversationId, StoreError> {
    let _held = conversations.lines.locked()?;
    let (line, opens) = LineEvent::begin(writer(), first, conversations.entropy)
        .map_err(|failed| StoreError::not_appended("conversation store", &failed))?;
    let id = line.begun_id();
    if keys(conversations)?.contains(&id) {
        return Err(StoreError::Taken {
            what: "conversation",
            name: id.to_string(),
        });
    }
    for key in &opens {
        conversations.keys.set(&key.name(), key)?;
    }
    conversations.lines.append(line)?;
    Ok(id)
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

/// The line `owed` answers on conversation `id`'s reading, appended under the lines' lock, where
/// one is still owed: a line two writers both owe is written once. Whether one was written.
///
/// # Errors
/// [`push`]'s.
pub fn push_owed(
    conversations: Conversations<'_>,
    id: &ConversationId,
    owed: impl FnOnce(&Transcript) -> Option<Entry>,
) -> Result<bool, StoreError> {
    let _held = conversations.lines.locked()?;
    let reading = read(conversations, id)?;
    let Some(entry) = owed(&reading) else {
        return Ok(false);
    };
    write(conversations, id, reading.lines().len(), &entry)?;
    Ok(true)
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

// The plain readings the fold is tested against, never production.
#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/plain_reading.rs"]
mod plain_reading;
#[cfg(any(test, feature = "nothing"))]
pub use plain_reading::{ReadFailed, of_objects, opened};
