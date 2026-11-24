//! The conversations as one prodrome at the [`LineEvent`] schema, keyed by conversation.
//!
//! Every line is written through [`push`] or [`start`], which take its words through the
//! redaction door and lock each slot under a fresh key before the store sees it.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_dag::registers::Node;
use prodrome_data::event::{Actor, Envelope, Hash};
use prodrome_data::literal::Datetime;
use prodrome_data::schema::Schema as _;

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
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::{
    ConversationState, Covered, Entry, NotAConversation, RunId, Said, Stored,
};
use suzatary_conversations::secretary::taint::{self, Taint};
use suzatary_conversations::secretary::turn_shape::TurnShape;
use suzatary_inbox::secretary::stamp::Stamp;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub name: Hash,
    pub line: LineEvent,
    pub entry: Entry,
    /// The positions, in [`Transcript::lines`], of the lines it rests on directly.
    pub rests_on: Vec<usize>,
}

/// One line of one conversation as a node holds it; its parents may include objects that are no
/// line of the conversation.
#[derive(Debug, Clone, Copy)]
pub struct Write<'n> {
    pub name: &'n Hash,
    pub line: &'n LineEvent,
    pub parents: &'n [Hash],
}

impl Transcript {
    /// One conversation's writes, in any order, put in causal order, each opened by the key
    /// `slots` answers for its name, and folded.
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
        let mut lines: Vec<Written> = order
            .into_iter()
            .map(|(at, rests_on)| Written {
                name: writes[at].0.name.clone(),
                line: writes[at].0.line.clone(),
                entry: writes[at].1.clone(),
                rests_on,
            })
            .collect();
        judged(&mut lines);
        let turns: Vec<usize> = lines
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, w)| matches!(w.entry, Entry::Said { .. } | Entry::Stopped { .. }))
            .map(|(at, _)| at)
            .collect();
        // An edit's `Again` folds at its own question's place, where it takes effect: a session
        // line written between them is dropped with what the edit sets aside.
        let edits = takers(lines.len(), |at| &lines[at].line, |at| &lines[at].rests_on);
        let mut takes = vec![None; lines.len()];
        for (at, edit) in edits.iter().enumerate() {
            if let EditAt::TakenBy(then) = *edit {
                takes[then] = Some(at);
            }
        }
        let stored = Stored::of_read(
            id,
            (0..lines.len())
                .filter(|&at| edits[at] == EditAt::Neither)
                .flat_map(|at| takes[at].into_iter().chain([at]))
                .map(|at| (&lines[at].entry, set_aside(&lines, &turns, at))),
        )?;
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

    /// The positions of the lines named `names` and of every line they rest on, directly or not.
    #[must_use]
    pub(crate) fn beneath_names(&self, names: &[Hash]) -> BTreeSet<usize> {
        let mut found = BTreeSet::new();
        let mut pending: Vec<usize> = self
            .lines
            .iter()
            .enumerate()
            .filter(|(_, written)| names.contains(&written.name))
            .map(|(at, _)| at)
            .collect();
        while let Some(at) = pending.pop() {
            if found.insert(at) {
                pending.extend(&self.lines[at].rests_on);
            }
        }
        found
    }

    /// The positions of every line that descends from the line at `from`, that line left out.
    #[must_use]
    pub(crate) fn descending(&self, from: usize) -> BTreeSet<usize> {
        let mut found = BTreeSet::from([from]);
        for at in from + 1..self.lines.len() {
            if self.lines[at]
                .rests_on
                .iter()
                .any(|parent| found.contains(parent))
            {
                found.insert(at);
            }
        }
        found.remove(&from);
        found
    }

    /// The turn the line at `line` says, by its index; none for a line that says no turn.
    #[must_use]
    pub(crate) fn turn_of(&self, line: usize) -> Option<usize> {
        self.turns.iter().position(|&at| at == line)
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

    /// Every line's entry, in causal order.
    #[must_use]
    pub fn entries(&self) -> Vec<Entry> {
        self.lines
            .iter()
            .map(|written| written.entry.clone())
            .collect()
    }

    #[must_use]
    pub fn stored(&self) -> &Stored {
        &self.stored
    }

    #[must_use]
    pub fn into_stored(self) -> Stored {
        std::sync::Arc::unwrap_or_clone(self.stored)
    }

    #[must_use]
    pub(crate) fn shared(&self) -> &std::sync::Arc<Stored> {
        &self.stored
    }

    /// The owed register: every live turn the claims do not read as settled, with its index. A
    /// person's or an act's turn is settled by a closed round that covered it, or by a stop or a
    /// close with no round open that rests on it; a model's or a verb's by any close resting on it.
    /// It reads descent and claims, not position, so it is the same for every arrival order.
    #[must_use]
    pub fn owed(&self) -> Vec<(usize, &Said)> {
        self.stored
            .live()
            .into_iter()
            .filter(|(turn, _)| self.claims.standing(self.turns[*turn]) != Standing::Settled)
            .collect()
    }

    /// Whether a turn that wakes a model is owed.
    #[must_use]
    pub fn owes_a_round(&self) -> bool {
        self.owed()
            .iter()
            .any(|(_, said)| suzatary_conversations::secretary::queue::wakes(said))
    }

    /// Each live turn owed a round that no round covers yet, by index.
    #[must_use]
    pub fn queued(&self) -> BTreeSet<usize> {
        self.stored
            .live()
            .into_iter()
            .filter(|(turn, _)| {
                let line = self.turns[*turn];
                matches!(self.marks[line], Mark::Request)
                    && self.claims.standing(line) == Standing::Queued
            })
            .map(|(turn, _)| turn)
            .collect()
    }

    /// The model's turns that answered turn `turn`, by index, in order: those the last round to
    /// cover it wrote while it was open, read off the claims. None while no round covered it.
    #[must_use]
    pub fn replies_to(&self, turn: usize) -> Option<Vec<usize>> {
        let line = *self.turns.get(turn)?;
        let round = self
            .claims
            .rounds()
            .iter()
            .rev()
            .find(|round| round.covers.contains(&line))?;
        let closing = match round.end {
            crate::secretary::claim::End::Closed(at) => Some(at),
            crate::secretary::claim::End::Open | crate::secretary::claim::End::Withdrawn => None,
        };
        Some(
            round
                .covers
                .iter()
                .copied()
                .chain(closing)
                .filter_map(|at| self.turn_of(at))
                .filter(|&at| matches!(self.stored.turns[at].turn, Turn::Model { .. }))
                .collect(),
        )
    }

    /// The conversation's answering state, read off the claims: thinking while a round is open,
    /// queued while a turn that wakes a model is owed and no round covers it, idle otherwise. A
    /// round whose run is gone reads thinking until the box's close settles it.
    #[must_use]
    pub fn state(&self) -> ConversationState {
        let open = self
            .claims
            .rounds()
            .iter()
            .any(|round| round.end == crate::secretary::claim::End::Open);
        let waiting = self
            .queued()
            .into_iter()
            .any(|turn| suzatary_conversations::secretary::queue::wakes(&self.stored.turns[turn]));
        if open {
            ConversationState::Thinking
        } else if waiting {
            ConversationState::Queued
        } else {
            ConversationState::Idle
        }
    }

    /// The name of the line of each turn, by index.
    #[must_use]
    pub fn turn_lines(&self) -> Vec<&Hash> {
        self.turns
            .iter()
            .map(|&line| &self.lines[line].name)
            .collect()
    }

    /// The runs whose rounds the line named `line` ends: for a stop, every round open before it in
    /// this order.
    #[must_use]
    pub fn ended_by(&self, line: &Hash) -> BTreeSet<RunId> {
        let Some(at) = self.lines.iter().position(|written| written.name == *line) else {
            return BTreeSet::new();
        };
        self.claims
            .rounds()
            .iter()
            .filter(|round| round.end == crate::secretary::claim::End::Closed(at))
            .map(|round| round.run.clone())
            .collect()
    }

    #[must_use]
    pub(crate) fn claims(&self) -> &Claims<RunId> {
        &self.claims
    }

    /// Every live request (a person's or an act's turn, or a model's or a verb's) as a claim step
    /// at `now` reads it.
    #[must_use]
    pub(crate) fn asked(&self, now: Datetime) -> Vec<Asked> {
        let now = prodrome_data::instant::instant_of(now);
        let mut afresh: BTreeMap<&Hash, Datetime> = BTreeMap::new();
        for (place, written) in self.lines.iter().enumerate() {
            if let Entry::Again { at, question, .. } = &written.entry {
                if inert(&self.lines, place) {
                    continue;
                }
                let latest = afresh.entry(question).or_insert(*at);
                *latest = (*latest).max(*at);
            }
        }
        self.stored
            .live()
            .into_iter()
            .map(|(turn, said)| (self.turns[turn], said))
            .filter(|(line, _)| matches!(self.marks[*line], Mark::Request | Mark::Answer))
            .map(|(at, said)| {
                let asked = afresh
                    .get(&self.lines[at].name)
                    .map_or(said.at, |again| said.at.max(*again));
                (at, said, asked)
            })
            .map(|(at, said, asked)| Asked {
                at,
                wakes: if suzatary_conversations::secretary::queue::wakes(said) {
                    Wakes::Model
                } else {
                    Wakes::RidesAlong
                },
                bound: if now - prodrome_data::instant::instant_of(asked) <= ANSWERED_WITHIN {
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
    Written,
    /// The round was closed already, so nothing was written.
    Answered,
}

/// [`push`] each of `entries`, a round's lines, in one hold of the lock, while the round of `run`
/// is open: a round that another line closed writes nothing more.
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

/// Whether every todo some references name is one a trusted actor confirmed, as the chains read
/// now.
pub type Confirms<'a> = dyn Fn(&[suzatary_base::reference::Reference]) -> bool + 'a;

/// How long after a request is made a round may still claim it: past it, the box closes it
/// unanswered.
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
/// round runs), written for the round of `run`. A covered turn's taint is the box's judgment of
/// it now.
pub fn claim(
    conversations: Conversations<'_>,
    id: &ConversationId,
    run: &RunId,
    now: Datetime,
    retry: Retry,
    confirms: &Confirms<'_>,
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
    write(
        conversations,
        id,
        held,
        &read.began(&covers, run, now, confirms),
    )?;
    Ok(Claimed::Began)
}

/// Before the round of `run` asks again, under the lines' lock: a `Began` of its run covering
/// every request queued, when there is any. A retry round takes none.
pub fn join(
    conversations: Conversations<'_>,
    id: &ConversationId,
    run: &RunId,
    now: Datetime,
    confirms: &Confirms<'_>,
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
        &read.began(&covers, run, now, confirms),
    )
}

impl Transcript {
    /// The lines the claim step of a start at `now` claims: a retry of a lost round, or a first
    /// claim that covers a person's question. A conversation owed only an act, a note or the
    /// box's close is not resumed.
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

    /// The claim of `run` over the lines at `covers`, each judged as the box reads it now: the
    /// box's own turn by its stamp, another writer's by the stamp it was written with, made
    /// `Unconfirmed` where `confirms` says a todo it names is not confirmed.
    fn began(
        &self,
        covers: &[usize],
        run: &RunId,
        now: Datetime,
        confirms: &Confirms<'_>,
    ) -> Entry {
        let turn_of = |line: usize| self.turns.iter().position(|&at| at == line);
        let judged = |line: usize| -> Option<Taint> {
            let said = &self.stored.turns[turn_of(line)?];
            let written = &self.lines[line];
            let LineShape::Said {
                by: Some(Stamp::Actor(speaker)),
                ..
            } = written.line.shape()
            else {
                return Some(taint::turn(said));
            };
            if by_the_box(written.line.actor()) {
                return Some(taint::turn(said));
            }
            let references = match &said.turn {
                Turn::Person { references, .. } => references.as_slice(),
                Turn::Model { .. } | Turn::Tool { .. } | Turn::Act { .. } | Turn::Erased { .. } => {
                    &[]
                }
            };
            let by = if confirms(references) {
                Stamp::Actor(speaker.clone())
            } else {
                Stamp::Unconfirmed(speaker.clone())
            };
            Some(taint::turn(&Said {
                by: Some(by),
                ..said.clone()
            }))
        };
        Entry::Began {
            at: now,
            covers: covers
                .iter()
                .filter_map(|&line| {
                    Some(Covered {
                        line: self.lines[line].name.clone(),
                        taint: judged(line)?,
                    })
                })
                .collect(),
            run: run.clone(),
        }
    }
}

/// Every conversation a start at `now` resumes, oldest id first: one whose claim step would retry a
/// lost round, or claim a person's question.
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
            | LineShape::Resealed { .. }
            | LineShape::Again { .. } => Mark::Other,
        })
        .collect()
}

/// The turns the line at `at` sets aside, by index, where it is an `Again` whose question is a line
/// here; none for any other line.
fn set_aside(lines: &[Written], turns: &[usize], at: usize) -> Vec<usize> {
    let Entry::Again { question, how, .. } = &lines[at].entry else {
        return Vec::new();
    };
    let Some(question) = lines.iter().position(|written| written.name == *question) else {
        return Vec::new();
    };
    let aside = crate::secretary::again::aside(
        |line| &lines[line].rests_on,
        |line, again| {
            takes_up(
                &lines[line].line,
                &lines[line].rests_on,
                again,
                &lines[again].line,
            )
        },
        lines.len(),
        question,
        at,
        *how,
    );
    turns
        .iter()
        .enumerate()
        .filter(|(_, line)| aside.contains(line))
        .map(|(turn, _)| turn)
        .collect()
}

fn edit(line: &LineEvent) -> bool {
    matches!(
        line.shape(),
        LineShape::Again {
            how: suzatary_conversations::secretary::store::Rewind::Edit,
            ..
        }
    )
}

/// Whether `later`, resting on the lines at `rests_on` (positions among the conversation's lines,
/// the genesis left out), takes up the edit `again` at `at`.
fn takes_up(later: &LineEvent, rests_on: &[usize], at: usize, again: &LineEvent) -> bool {
    rests_on == [at] && edit(again) && crate::secretary::step::Step::of(later.shape()).takes_up()
}

/// What a line is to the edits: an edit's `Again` nothing takes up, one the line at a position
/// takes up, or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditAt {
    Neither,
    Inert,
    TakenBy(usize),
}

/// What each of `len` lines is to the edits, by position: the first line that takes an edit's
/// `Again` up ([`takes_up`]) takes it.
fn takers<'l>(
    len: usize,
    line: impl Fn(usize) -> &'l LineEvent,
    rests_on: impl Fn(usize) -> &'l [usize],
) -> Vec<EditAt> {
    let mut edits: Vec<EditAt> = (0..len)
        .map(|at| {
            if edit(line(at)) {
                EditAt::Inert
            } else {
                EditAt::Neither
            }
        })
        .collect();
    for later in 0..len {
        if let [at] = rests_on(later)
            && let Some(slot) = edits.get_mut(*at)
            && *slot == EditAt::Inert
            && takes_up(line(later), rests_on(later), *at, line(*at))
        {
            *slot = EditAt::TakenBy(later);
        }
    }
    edits
}

/// The edits' `Again` lines among `lines` (a name, a line, the names it rests on) that no line
/// takes up.
#[must_use]
pub fn inert_edits<'l>(lines: &[(&'l Hash, &'l LineEvent, &'l [Hash])]) -> BTreeSet<&'l Hash> {
    let index: BTreeMap<&Hash, usize> = lines
        .iter()
        .enumerate()
        .map(|(at, (name, _, _))| (*name, at))
        .collect();
    let rests_on: Vec<Vec<usize>> = lines
        .iter()
        .map(|(_, _, parents)| {
            parents
                .iter()
                .filter_map(|parent| index.get(parent).copied())
                .collect()
        })
        .collect();
    takers(lines.len(), |at| lines[at].1, |at| &rests_on[at])
        .into_iter()
        .zip(lines)
        .filter(|(edit, _)| *edit == EditAt::Inert)
        .map(|(_, (name, _, _))| *name)
        .collect()
}

/// The position of the line that takes up the edit's `Again` at `at`; none for any other line.
fn taker(lines: &[Written], at: usize) -> Option<usize> {
    (at + 1..lines.len()).find(|&later| {
        takes_up(
            &lines[later].line,
            &lines[later].rests_on,
            at,
            &lines[at].line,
        )
    })
}

/// Whether the line at `at` is an edit's `Again` no line takes up: it reads as nothing.
pub(crate) fn inert(lines: &[Written], at: usize) -> bool {
    edit(&lines[at].line) && taker(lines, at).is_none()
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
/// from, where `slots` opens that reseal's one slot, or where a later reseal supersedes the
/// reseal's own key. A reseal whose key is not held, and that nothing later supersedes, supersedes
/// nothing, so a store restored without its keys keeps reading the old seal until a run seals it
/// again. Monotone in the lines and in the keys held.
#[must_use]
pub fn superseded<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
    slots: &impl Slots,
) -> BTreeSet<KeyName> {
    superseded_among(lines, &|_| true, slots)
}

/// The keys of [`superseded`] that `asked` admits; no other key opens a reseal.
pub(crate) fn superseded_among<'l>(
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
/// opens, or whose own key a later reseal supersedes in turn.
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
/// per set of heads; whether a key is superseded is asked of its own conversation's reseals alone.
#[derive(Debug, Default)]
pub struct Retired {
    pub erased: BTreeMap<KeyName, Erasure>,
    resealed: BTreeMap<ConversationId, BTreeMap<KeyName, Vec<Hash>>>,
}

impl Retired {
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
/// first, and among those under the same rules the newest first. Where a slot's own seal does not
/// open, it is read from the first of these that does, so a slot whose every reseal is under older
/// rules reads its newest, which the current rules scrub least.
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

/// How many reseals down from the slot's first seal `reseal` stands: one when it was sealed again
/// from the first seal `of`, one more than the reseal whose own key it was sealed from otherwise.
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
    pub erased: &'a BTreeMap<KeyName, Erasure>,
    /// The keys a reseal that opens here supersedes.
    pub superseded: &'a BTreeSet<KeyName>,
    pub reseals: &'a BTreeMap<KeyName, Vec<&'a LineEvent>>,
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

/// An object of the lines beside its name.
pub trait Named<'l> {
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

fn lines_of<'l>(
    objects: impl IntoIterator<Item = impl Named<'l>>,
) -> impl Iterator<Item = (&'l Hash, &'l LineEvent)> {
    objects.into_iter().filter_map(|object| {
        let (name, object) = object.named();
        object.event().map(|line| (name, line))
    })
}

/// Every slot whose key an erasure line of its own conversation names, or a reseal of it that
/// `slots` opens supersedes ([`superseded`]), as the line, the slot and its key's name. Monotone in
/// the lines and the keys held.
#[must_use]
pub fn dead<'l>(
    lines: impl IntoIterator<Item = (&'l Hash, &'l LineEvent)>,
    slots: &impl Slots,
) -> Vec<(&'l LineEvent, usize, &'l KeyName)> {
    dead_among(lines, &|_| true, slots)
}

/// The slots of [`dead`] whose key `asked` admits; no other key opens a reseal.
pub(crate) fn dead_among<'l>(
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

/// The answer blocks slot `slot`'s words were rendered to, opened by `key`: what a replica forgets
/// before it destroys the key, since nothing finds them once the words are gone. A slot under older
/// rules names the blocks of its words as locked and as the current rules read them; the words as
/// locked are shown to no one.
#[must_use]
pub fn fed(line: &LineEvent, slot: usize, key: &TextKey) -> Vec<suzatary_base::answer::Renderable> {
    rendered(
        [line.unscrubbed(slot, key), line.open(slot, key)]
            .iter()
            .flatten(),
    )
}

/// The answer blocks `words` were rendered to, each once.
pub fn rendered<'w>(
    words: impl IntoIterator<Item = &'w LineWords>,
) -> Vec<suzatary_base::answer::Renderable> {
    let mut fed: Vec<suzatary_base::answer::Renderable> = Vec::new();
    for words in words {
        if let LineWords::Model { text } = words {
            for block in suzatary_base::answer::blocks(text) {
                let block = suzatary_base::answer::Renderable::of(&block.source);
                if !fed.contains(&block) {
                    fed.push(block);
                }
            }
        }
    }
    fed
}

/// Every key the objects' lines name that an erasure of the same conversation names, each with why.
/// A key an erasure names in another conversation is no key of its, and erases nothing.
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

/// One conversation's lines in the order every reading takes ([`sequence::sequenced`]), each with
/// the positions in that order of the lines it rests on.
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
/// is that conversation's refusal and no other's.
pub fn each(conversations: Conversations<'_>) -> Result<Vec<(ConversationId, Read)>, StoreError> {
    conversations.reads().fold(HeldLines::transcripts)
}

/// What `verify` reports of the conversations, read against `inbox` and by their shapes alone,
/// since whether a conversation folds never rests on its words: the store's own findings, every
/// conversation whose lines do not fold, and every proposal line naming a record the inbox does not
/// hold, by its reference.
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
/// [`StoreError::Absent`] for an id never started.
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
pub fn lines(
    conversations: Conversations<'_>,
    id: &ConversationId,
) -> Result<Vec<Entry>, StoreError> {
    Ok(read(conversations, id)?.entries())
}

/// Every conversation's id, sorted.
pub fn keys(conversations: Conversations<'_>) -> Result<Vec<ConversationId>, StoreError> {
    let nodes = conversations.lines.nodes()?;
    Ok(writes(&nodes).into_keys().cloned().collect())
}

/// Destroy every key the key store still holds that an erasure line names, that a reseal which
/// opens supersedes, or that seals a reseal waiting outside the history whose slot the history
/// reads without it, each once every holder of its words is emptied ([`Holder::ALL`]), and say how
/// many. A key goes only where what it opens is held elsewhere or erased, checked. The sweep is
/// monotone: it takes no hold, and running it twice, or on two replicas, destroys what running it
/// once does. Taking no hold, it can see a batch's child placed before its parent while a migration
/// places its lines; the slot reads the same, and the next run seals it again.
///
/// # Errors
/// [`StoreError::Unwritable`] naming every key kept because a copy of its words may still be on
/// disk; the next sweep tries again.
pub fn sweep(conversations: Conversations<'_>) -> Result<usize, StoreError> {
    sweep_knowing(conversations, &Known::default())
}

/// What a writer that has just opened or sealed slots knows of them, so the sweep after it opens
/// none of them again. The sweep trusts it, so only the migration makes one.
#[derive(Debug, Default)]
pub(crate) struct Known {
    /// The blocks the words under each key were rendered to.
    pub(crate) fed: BTreeMap<KeyName, Vec<suzatary_base::answer::Renderable>>,
    /// The keys of seals the writer made: each opens its slot, whose words it sealed.
    pub(crate) sealed: BTreeSet<KeyName>,
}

/// [`sweep`], asking `known` before opening a slot.
pub(crate) fn sweep_knowing(
    conversations: Conversations<'_>,
    known: &Known,
) -> Result<usize, StoreError> {
    let graph = conversations.lines.graph()?;
    // A key neither holds has nothing left to empty; an unreadable store is asked, and refuses.
    let held = |name: &KeyName| {
        conversations.opened.holds(name) || !matches!(conversations.keys.get(name), Ok(None))
    };
    let keys = |name: &KeyName| conversations.reads().key(name);
    let waiting = waiting_reseals(conversations.lines, &graph, &keys)?;
    // A waiting reseal of an erased slot is a dead slot: every holder of its words is emptied.
    let erased = waiting
        .erased
        .iter()
        .filter_map(|line| line.keys().next().map(|key| (line, 0, key)));
    destroy(
        conversations,
        &sessions(lines_of(graph.objects())),
        known,
        &waiting.read_elsewhere,
        dead_among(
            lines_of(graph.objects()),
            &held,
            &Sealed {
                sealed: &known.sealed,
                slots: &keys,
            },
        )
        .into_iter()
        .chain(erased),
    )
}

/// The key of every reseal outside the history whose slot the history reads without it: a seal
/// of that slot in the history opens as `slots` reads it, or an erasure in the history names the
/// slot. A reseal is also left out when a line beneath it does not read, for a cause that can be
/// mended; then the history holds no other seal that opens, and the key stays.
fn waiting_reseals(
    lines: &dyn Prodrome<LineEvent>,
    history: &suzatary_store::store::Graph<LineEvent>,
    slots: &impl Slots,
) -> Result<Waiting, StoreError> {
    let vocabulary = crate::secretary::line_event::LineVocabulary;
    let by = by_conversation(lines_of(history.objects()));
    let erased = |waiting: &LineEvent, of: &KeyName| {
        by.get(waiting.conversation())
            .is_some_and(|held| erasures(held.iter().copied()).contains_key(of))
    };
    let read_elsewhere = |waiting: &LineEvent, of: &KeyName| {
        let Some(held) = by.get(waiting.conversation()) else {
            return false;
        };
        held.iter().any(|(_, line)| {
            if let LineShape::Resealed { of: sealed, .. } = line.shape() {
                sealed == of && line.slot_opens(0, slots)
            } else {
                line.keys()
                    .position(|key| key == of)
                    .is_some_and(|slot| line.slot_opens(slot, slots))
            }
        })
    };
    let mut waiting = Waiting::default();
    let excluded = lines.excluded()?;
    let left_out = excluded
        .iter()
        .filter_map(|name| lines.literals(std::slice::from_ref(name)).ok()?.pop()?)
        .filter_map(|print| {
            prodrome_data::event::parse_envelope::<LineEvent>(&vocabulary, &print).ok()
        })
        .filter_map(|object| object.event().cloned());
    for line in left_out {
        let LineShape::Resealed { of, .. } = line.shape() else {
            continue;
        };
        if erased(&line, of) {
            waiting.erased.push(line);
        } else if read_elsewhere(&line, of) {
            waiting.read_elsewhere.extend(line.keys().cloned());
        }
    }
    Ok(waiting)
}

/// The reseals waiting outside the history that a sweep destroys the key of.
#[derive(Default)]
struct Waiting {
    /// Of a slot an erasure in the history names: dead slots, each holder of their words emptied.
    erased: Vec<LineEvent>,
    /// The keys of those whose slot a seal in the history opens: the words stay readable there.
    read_elsewhere: Vec<KeyName>,
}

/// `slots`, answering that a slot opens without opening it where its key is one `sealed` names,
/// since whoever sealed under that key sealed that slot.
struct Sealed<'a, S> {
    sealed: &'a BTreeSet<KeyName>,
    slots: &'a S,
}

impl<S: Slots> Slots for Sealed<'_, S> {
    fn words(
        &self,
        name: &KeyName,
        open: &dyn Fn(&TextKey) -> Option<Words<LineWords>>,
    ) -> Words<LineWords> {
        self.slots.words(name, open)
    }

    fn opens(
        &self,
        name: &KeyName,
        read: &dyn Fn() -> Words<LineWords>,
        unlocks: &dyn Fn(&TextKey) -> bool,
    ) -> bool {
        self.slots.opens(name, read, &|key| {
            self.sealed.contains(&key.name()) || unlocks(key)
        })
    }
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

/// [`sweep`] of `slots` and of the `waiting` reseals' keys: every holder but the key store emptied
/// of each, then every key whose holders are empty destroyed in one batch.
fn destroy<'l>(
    conversations: Conversations<'_>,
    sessions: &BTreeMap<&ConversationId, BTreeSet<(&SessionId, &str)>>,
    known: &Known,
    waiting: &'l [KeyName],
    slots: impl IntoIterator<Item = (&'l LineEvent, usize, &'l KeyName)>,
) -> Result<usize, StoreError> {
    let mut emptied: Vec<&KeyName> = Vec::new();
    // Its slot's words stay readable from the history, so only the key itself goes.
    for name in waiting {
        if conversations.keys.get(name)?.is_some() {
            conversations.opened.forget(name);
            emptied.push(name);
        }
    }
    let mut kept = Vec::new();
    for (line, slot, name) in slots {
        let Some(key) = conversations.keys.get(name)? else {
            conversations.opened.forget(name);
            continue;
        };
        // A copy left on disk keeps the key, so the next sweep can find the copy again.
        let other_holders = Holder::ALL.into_iter().try_for_each(|holder| match holder {
            Holder::Opened => {
                conversations.opened.forget(name);
                Ok(())
            }
            Holder::Renders => known
                .fed
                .get(name)
                .map_or_else(|| fed(line, slot, &key), Clone::clone)
                .iter()
                .try_for_each(|block| conversations.renders.forget(block)),
            Holder::ModelSessions => sessions
                .get(line.conversation())
                .into_iter()
                .flatten()
                .try_for_each(|(id, cwd)| conversations.sessions.forget(id, cwd)),
            // The keys are destroyed below, together, once every other holder of each is
            // emptied; the rest are out of the box's reach, and the erase question names them.
            Holder::Keys | Holder::Vault | Holder::DataHistory | Holder::DeviceLines => Ok(()),
        });
        match other_holders {
            Ok(()) => emptied.push(name),
            Err(failed) => kept.push(format!("{name}: {failed}")),
        }
    }
    let destroyed = match conversations.keys.remove_all(&emptied) {
        Ok(()) => emptied.len(),
        Err(failed) => {
            kept.extend(emptied.iter().map(|name| format!("{name}: {failed}")));
            0
        }
    };
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
/// an erasure names a slot by the key it was first sealed under, which erases every seal of it.
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
    // the next sweep.
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
/// [`StoreError::Taken`] for an id already started.
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

/// Begin a new conversation with `first`, its id a function of that line ([`LineEvent::begun_id`]);
/// answers the id.
///
/// # Errors
/// [`StoreError::Taken`] where the store holds a line of that id already, or an entry the door
/// or the schema refuses.
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
/// [`StoreError::Absent`] for an id never started, or an entry the door or the schema refuses.
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

/// Each turn another writer wrote, read with the stamp its claims judged it to carry: as written
/// where every claim covering it judged it clean, and `Unconfirmed`, which every reader reads as
/// tainted, where one judged it tainted or none covers it yet. The box's own lines read as they
/// were written.
fn judged(lines: &mut [Written]) {
    let judgments = judgments(lines.iter().map(|written| &written.entry));
    for written in lines.iter_mut() {
        judge(
            &written.name,
            written.line.actor(),
            &mut written.entry,
            &judgments,
        );
    }
}

/// What the claims among `entries` judged of each line they cover: the join of every judgment.
#[must_use]
pub fn judgments<'e>(entries: impl IntoIterator<Item = &'e Entry>) -> BTreeMap<Hash, Taint> {
    let mut judgments: BTreeMap<Hash, Taint> = BTreeMap::new();
    for entry in entries {
        if let Entry::Began { covers, .. } = entry {
            for covered in covers {
                let taint = judgments.entry(covered.line.clone()).or_default();
                *taint = taint.join(covered.taint);
            }
        }
    }
    judgments
}

/// `entry`, the line `name` that `actor` wrote, read with the stamp `judgments` give it: every
/// reader of a turn's taint reads it so.
pub fn judge(
    name: &Hash,
    actor: &prodrome_data::event::Actor,
    entry: &mut Entry,
    judgments: &BTreeMap<Hash, Taint>,
) {
    if by_the_box(actor) {
        return;
    }
    let judgment = judgments.get(name).copied();
    if let Entry::Said {
        by: Some(by @ Stamp::Actor(_)),
        ..
    } = entry
        && judgment != Some(Taint::Clean)
        && let Stamp::Actor(speaker) = by.clone()
    {
        *by = Stamp::Unconfirmed(speaker);
    }
}
