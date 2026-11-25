//! A conversation's rounds without their words: the turn that opens each and when, what the stream
//! counts and chooses by, and the lines that hold each round's words. `of_steps` (from the
//! skeleton) and [`of_stored`] (from the whole conversation) agree for every conversation
//! (`tests/transcript.rs`), so the stream chooses its rounds before it reads one.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_data::event::Hash;
use prodrome_data::literal::Datetime;

use crate::secretary::line_shape::unkinded;
use crate::secretary::step::Step;
use crate::secretary::targets::rounds;
use crate::wire::lines::LineWriter;
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::store::{Kind, NotAConversation, Stored};
use suzatary_memory::secretary::locked_text::KeyName;

/// Each live round, oldest first, or why the lines are no conversation.
pub type Outline = Result<Rounds, NotAConversation>;

/// The turn that opens a round, and when it was said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opening {
    /// Its index among the conversation's turns.
    pub turn: usize,
    pub at: Datetime,
}

/// One line as the outline reads it.
#[derive(Debug, Clone, Copy)]
pub struct Line<'a> {
    pub name: &'a Hash,
    pub at: Datetime,
    pub parents: &'a [Hash],
    /// The key of each slot of its words.
    pub keys: &'a [KeyName],
    pub step: &'a Step,
    pub writer: &'a LineWriter,
}

/// A conversation's live rounds, and where the words each reads are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rounds {
    live: Vec<Round>,
    /// The line that opens the conversation, when it was written, and what the conversation is
    /// for, as the fold reads it ([`unkinded`] where the opening stores no kind).
    opening: Hash,
    opened: Datetime,
    kind: Kind,
    /// The line whose words title the conversation.
    title: Hash,
    /// Every turn, set aside or not, oldest first: when each was said.
    said: Vec<Datetime>,
    /// Each proposal line, in the order made.
    proposals: Vec<Proposed>,
    /// The turns that are the box's own notes ([`Step::Noted`]), which say no words a list reads.
    noted: BTreeSet<Hash>,
    /// The slot keys of every line a round may read.
    keys: BTreeMap<Hash, Vec<KeyName>>,
    /// The reseals of each slot, by the slot's first key.
    reseals: BTreeMap<KeyName, BTreeSet<Hash>>,
    erasures: BTreeMap<KeyName, BTreeSet<Hash>>,
}

/// A proposal line, and how many turns stood before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposed {
    pub line: Hash,
    pub after_turn: usize,
}

/// One live round: its opening, and each of its turns by index with the line that says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Round {
    pub opening: Opening,
    /// Its turns, the opening first.
    pub turns: Vec<(usize, Hash)>,
    pub voices: Voices,
}

/// Who says a round's turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voices {
    /// The box alone, in its own notes ([`Step::Noted`]): the line above a card the Inbox draws,
    /// which the stream leaves out.
    BoxAlone,
    /// Anyone else too.
    Others,
}

impl Voices {
    /// Who says a line taking `step`.
    fn of(step: &Step) -> Self {
        match step {
            Step::Noted => Voices::BoxAlone,
            Step::Opened { .. }
            | Step::Asks
            | Step::Edits
            | Step::Answers
            | Step::Rewound { .. }
            | Step::Again { .. }
            | Step::Proposed
            | Step::Coding { .. }
            | Step::Titled
            | Step::Resealed { .. }
            | Step::Erased { .. }
            | Step::Still
            | Step::Began { .. } => Voices::Others,
        }
    }

    /// Who says a round of these voices and then a turn of `then`.
    fn and(self, then: Self) -> Self {
        match (self, then) {
            (Voices::BoxAlone, Voices::BoxAlone) => Voices::BoxAlone,
            (Voices::BoxAlone | Voices::Others, Voices::Others)
            | (Voices::Others, Voices::BoxAlone) => Voices::Others,
        }
    }
}

impl Rounds {
    /// The rounds, oldest first.
    #[must_use]
    pub fn rounds(&self) -> &[Round] {
        &self.live
    }

    /// Each round's opening, oldest first.
    #[must_use]
    pub fn openings(&self) -> Vec<Opening> {
        self.live.iter().map(|round| round.opening).collect()
    }

    #[must_use]
    pub fn title(&self) -> &Hash {
        &self.title
    }

    #[must_use]
    pub fn opening(&self) -> &Hash {
        &self.opening
    }

    #[must_use]
    pub fn opened(&self) -> Datetime {
        self.opened
    }

    #[must_use]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// When each turn was said, oldest first, those set aside included.
    #[must_use]
    pub fn said(&self) -> &[Datetime] {
        &self.said
    }

    /// Each proposal line, in the order made.
    #[must_use]
    pub fn proposals(&self) -> &[Proposed] {
        &self.proposals
    }

    /// The line of each live turn that may say words a list reads, newest first: every one but
    /// the box's own notes, the one kind of turn the skeleton says says none.
    pub fn newest(&self) -> impl Iterator<Item = &Hash> {
        self.live
            .iter()
            .rev()
            .flat_map(|round| round.turns.iter().rev().map(|(_, line)| line))
            .filter(|line| !self.noted.contains(*line))
    }

    /// Every line whose words `round` reads: the title's and its turns' ([`Rounds::reads`]).
    #[must_use]
    pub fn objects(&self, round: &Round) -> BTreeSet<Hash> {
        self.reads(std::iter::once(&self.title).chain(round.turns.iter().map(|(_, line)| line)))
    }

    /// `lines`, every reseal of their slots, and every erasure naming a key of those, as a
    /// conversation's reading opens them.
    #[must_use]
    pub fn reads<'h>(&self, lines: impl IntoIterator<Item = &'h Hash>) -> BTreeSet<Hash> {
        let mut objects: BTreeSet<Hash> = lines.into_iter().cloned().collect();
        let own: Vec<&KeyName> = objects
            .iter()
            .flat_map(|line| self.keys.get(line).into_iter().flatten())
            .collect();
        let reseals: Vec<&Hash> = own
            .iter()
            .flat_map(|key| self.reseals.get(*key).into_iter().flatten())
            .collect();
        let sealed: Vec<&KeyName> = reseals
            .iter()
            .flat_map(|line| self.keys.get(*line).into_iter().flatten())
            .collect();
        let erasures: Vec<&Hash> = own
            .iter()
            .chain(&sealed)
            .flat_map(|key| self.erasures.get(*key).into_iter().flatten())
            .collect();
        objects.extend(reseals.into_iter().chain(erasures).cloned());
        objects
    }
}

/// The places in `order` the `Again` at `place` sets aside ([`crate::secretary::again::aside`]);
/// none where its question is no line here.
fn again_aside(
    lines: &[Line<'_>],
    order: &crate::secretary::sequence::Order,
    question: &Hash,
    place: usize,
    how: suzatary_conversations::secretary::store::Rewind,
) -> BTreeSet<usize> {
    let Some(question) = order.iter().position(|(at, _)| lines[*at].name == question) else {
        return BTreeSet::new();
    };
    crate::secretary::again::aside(
        |at| &order[at].1,
        |at, _| lines[order[at].0].step.takes_up(),
        order.len(),
        question,
        place,
        how,
    )
}

/// The outline of conversation `id` from its lines, in any order: put in the order the
/// conversation's own reading puts them, then folded as `Stored::fold` folds their turns and title.
pub(crate) fn of_steps(id: &ConversationId, lines: &[Line<'_>]) -> Outline {
    let order = crate::secretary::transcript::sequenced(
        &lines
            .iter()
            .map(|line| crate::secretary::sequence::Node {
                name: line.name,
                at: line.at,
                parents: line.parents,
                writer: *line.writer,
            })
            .collect::<Vec<_>>(),
    );
    let mut ordered = order.iter().map(|(at, _)| &lines[*at]).enumerate();
    let (opening, opened, kind) = match ordered.next().map(|(_, line)| line) {
        Some(Line {
            name,
            at,
            step: Step::Opened { kind },
            ..
        }) => (*name, *at, kind.unwrap_or_else(|| unkinded(id))),
        Some(_) | None => return Err(NotAConversation::Unopened(id.clone())),
    };
    let mut title = opening;
    let mut proposed: Vec<Proposed> = Vec::new();
    let mut turns: Vec<(Datetime, &Step, &Hash)> = Vec::new();
    let mut aside: BTreeSet<usize> = BTreeSet::new();
    let mut turn_at: Vec<Option<usize>> = vec![None; order.len()];
    let mut reseals: BTreeMap<KeyName, BTreeSet<Hash>> = BTreeMap::new();
    let mut erasures: BTreeMap<KeyName, BTreeSet<Hash>> = BTreeMap::new();
    for (place, line) in ordered {
        match line.step {
            Step::Asks | Step::Edits | Step::Noted | Step::Answers => {
                turn_at[place] = Some(turns.len());
                turns.push((line.at, line.step, line.name));
            }
            Step::Rewound { to } if *to > turns.len() => {
                return Err(NotAConversation::Unsaid {
                    id: id.clone(),
                    to: *to,
                });
            }
            Step::Rewound { to } => aside.extend(*to..turns.len()),
            Step::Again { question, how } => {
                aside.extend(
                    again_aside(lines, &order, question, place, *how)
                        .into_iter()
                        .filter_map(|at| turn_at[at]),
                );
            }
            Step::Proposed => proposed.push(Proposed {
                line: line.name.clone(),
                after_turn: turns.len(),
            }),
            Step::Coding { proposal } if *proposal >= proposed.len() => {
                return Err(NotAConversation::Phantom {
                    id: id.clone(),
                    index: *proposal,
                });
            }
            Step::Titled => title = line.name,
            Step::Resealed { of, .. } => {
                reseals
                    .entry(of.clone())
                    .or_default()
                    .insert(line.name.clone());
            }
            Step::Erased { keys } => {
                for key in keys {
                    erasures
                        .entry(key.clone())
                        .or_default()
                        .insert(line.name.clone());
                }
            }
            Step::Opened { .. } | Step::Still | Step::Coding { .. } | Step::Began { .. } => {}
        }
    }
    let keys = lines
        .iter()
        .filter(|line| !line.keys.is_empty())
        .map(|line| (line.name.clone(), line.keys.to_vec()))
        .collect();
    Ok(Rounds {
        live: live(&turns, &aside),
        opening: opening.clone(),
        opened,
        kind,
        title: title.clone(),
        said: turns.iter().map(|(at, _, _)| *at).collect(),
        proposals: proposed,
        noted: turns
            .iter()
            .filter(|(_, step, _)| **step == Step::Noted)
            .map(|(_, _, line)| (*line).clone())
            .collect(),
        keys,
        reseals,
        erasures,
    })
}

/// The live rounds of `turns`, oldest first, those `aside` names left out: each opens with a turn
/// that does not answer, or with the first live turn.
fn live(turns: &[(Datetime, &Step, &Hash)], aside: &BTreeSet<usize>) -> Vec<Round> {
    let live = turns
        .iter()
        .enumerate()
        .filter(|(turn, _)| !aside.contains(turn));
    let mut rounds: Vec<Round> = Vec::new();
    for (count, (turn, &(at, step, line))) in live.enumerate() {
        match rounds.last_mut() {
            Some(round) if count > 0 && *step == Step::Answers => {
                round.turns.push((turn, line.clone()));
                round.voices = round.voices.and(Voices::of(step));
            }
            Some(_) | None => rounds.push(Round {
                opening: Opening { turn, at },
                turns: vec![(turn, line.clone())],
                voices: Voices::of(step),
            }),
        }
    }
    rounds
}

/// The openings of a conversation read whole.
#[must_use]
pub fn of_stored(stored: &Stored) -> Vec<Opening> {
    rounds(stored)
        .iter()
        .map(|round| Opening {
            turn: round.opening.0,
            at: round.opening.1.at,
        })
        .collect()
}
