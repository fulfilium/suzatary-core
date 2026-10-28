//! A conversation's rounds without their words: the turn that opens each and when, what the stream
//! counts and chooses by, and the lines that hold each round's words. [`of_steps`] folds it from
//! each line's [`Step`], which the skeleton carries, and [`of_stored`] reads the openings off the
//! whole conversation; the two agree for every conversation (`tests/transcript.rs`), so the stream
//! chooses its rounds, and the objects it reads, before it reads one.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;
use prodrome_core::literal::Datetime;

use crate::secretary::locked_text::KeyName;
use crate::secretary::step::Step;
use crate::secretary::store::{ConversationId, NotAConversation, Stored};
use crate::secretary::topics::targets::rounds;
use crate::secretary::transcript;

/// Each live round, oldest first, or why the lines are no conversation.
pub type Outline = Result<Rounds, NotAConversation>;

/// The turn that opens a round, and when it was said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opening {
    /// Its index among the conversation's turns.
    pub turn: usize,
    /// When it was said.
    pub at: Datetime,
}

/// One line as the outline reads it.
#[derive(Debug, Clone, Copy)]
pub struct Line<'a> {
    /// Its name.
    pub name: &'a Hash,
    /// When it was written.
    pub at: Datetime,
    /// What it rests on.
    pub parents: &'a [Hash],
    /// The key of each slot of its words.
    pub keys: &'a [KeyName],
    /// What it does to the conversation's reading.
    pub step: &'a Step,
}

/// A conversation's live rounds, and where the words each reads are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rounds {
    live: Vec<Round>,
    /// The line whose words title the conversation.
    title: Hash,
    /// The slot keys of every line a round may read.
    keys: BTreeMap<Hash, Vec<KeyName>>,
    /// The reseals of each slot, by the slot's first key.
    reseals: BTreeMap<KeyName, BTreeSet<Hash>>,
    /// The erasures naming each key.
    erasures: BTreeMap<KeyName, BTreeSet<Hash>>,
}

/// One live round: its opening, and each of its turns by index with the line that says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Round {
    /// Its opening.
    pub opening: Opening,
    /// Its turns, the opening first.
    pub turns: Vec<(usize, Hash)>,
    /// Who says its turns.
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
            Step::Opened
            | Step::Asks
            | Step::Answers
            | Step::Rewound { .. }
            | Step::Proposed
            | Step::Coding { .. }
            | Step::Titled
            | Step::Resealed { .. }
            | Step::Erased { .. }
            | Step::Still => Voices::Others,
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

    /// The line whose words title the conversation.
    #[must_use]
    pub fn title(&self) -> &Hash {
        &self.title
    }

    /// Every line whose words `round` reads: the title's, its turns', every reseal of their slots,
    /// and every erasure naming a key of those, as a conversation's reading opens them. A slot is
    /// read from its own seal, its reseals and its erasures alone, so these lines open as the whole
    /// conversation opens them.
    #[must_use]
    pub fn objects(&self, round: &Round) -> BTreeSet<Hash> {
        let mut objects: BTreeSet<Hash> = std::iter::once(&self.title)
            .chain(round.turns.iter().map(|(_, line)| line))
            .cloned()
            .collect();
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

/// The outline of conversation `id` from its lines, in any order: put in the order the
/// conversation's own reading puts them, then folded as `Stored::fold` folds their turns and title.
///
/// # Errors
/// [`NotAConversation`], as the conversation's own fold refuses its lines.
pub fn of_steps(id: &ConversationId, lines: &[Line<'_>]) -> Outline {
    let order = transcript::causal(
        &lines
            .iter()
            .map(|line| (line.name, line.at, line.parents))
            .collect::<Vec<_>>(),
    );
    let mut ordered = order.iter().map(|(at, _)| &lines[*at]);
    let mut title = match ordered.next() {
        Some(first) if *first.step == Step::Opened => first.name,
        Some(_) | None => return Err(NotAConversation::Unopened(id.clone())),
    };
    // Each turn's instant, its step and its line; the spans set aside; how many proposals.
    let mut turns: Vec<(Datetime, &Step, &Hash)> = Vec::new();
    let mut aside: Vec<std::ops::Range<usize>> = Vec::new();
    let mut proposals = 0;
    let mut reseals: BTreeMap<KeyName, BTreeSet<Hash>> = BTreeMap::new();
    let mut erasures: BTreeMap<KeyName, BTreeSet<Hash>> = BTreeMap::new();
    for line in ordered {
        match line.step {
            Step::Asks | Step::Noted | Step::Answers => {
                turns.push((line.at, line.step, line.name));
            }
            Step::Rewound { to } if *to > turns.len() => {
                return Err(NotAConversation::Unsaid {
                    id: id.clone(),
                    to: *to,
                });
            }
            Step::Rewound { to } => aside.push(*to..turns.len()),
            Step::Proposed => proposals += 1,
            Step::Coding { proposal } if *proposal >= proposals => {
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
            Step::Opened | Step::Still | Step::Coding { .. } => {}
        }
    }
    let live = turns
        .iter()
        .enumerate()
        .filter(|(turn, _)| !aside.iter().any(|span| span.contains(turn)));
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
    let keys = lines
        .iter()
        .filter(|line| !line.keys.is_empty())
        .map(|line| (line.name.clone(), line.keys.to_vec()))
        .collect();
    Ok(Rounds {
        live: rounds,
        title: title.clone(),
        keys,
        reseals,
        erasures,
    })
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
