//! A conversation's rounds without their words: the turn that opens each and when, what the stream
//! counts and chooses by. [`of_steps`] folds it from each line's [`Step`], which the skeleton
//! carries, and [`of_stored`] reads it off the whole conversation; the two agree for every
//! conversation (`tests/transcript.rs`), so the stream chooses its rounds before it reads one.

use prodrome_core::event::Hash;
use prodrome_core::literal::Datetime;

use crate::secretary::step::Step;
use crate::secretary::store::{ConversationId, NotAConversation, Stored};
use crate::secretary::topics::targets::rounds;
use crate::secretary::transcript;

/// Each live round's opening, oldest first, or why the lines are no conversation.
pub type Outline = Result<Vec<Opening>, NotAConversation>;

/// The turn that opens a round, and when it was said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opening {
    /// Its index among the conversation's turns.
    pub turn: usize,
    /// When it was said.
    pub at: Datetime,
}

/// One line as the outline reads it: its name, when it was written, what it rests on and its step.
pub type Line<'a> = (&'a Hash, Datetime, &'a [Hash], Step);

/// The outline of conversation `id` from its lines, in any order: put in the order the
/// conversation's own reading puts them, then folded as `Stored::fold` folds their turns.
///
/// # Errors
/// [`NotAConversation`], as the conversation's own fold refuses its lines.
pub fn of_steps(id: &ConversationId, lines: &[Line<'_>]) -> Outline {
    let order = transcript::causal(
        &lines
            .iter()
            .map(|(name, at, parents, _)| (*name, *at, *parents))
            .collect::<Vec<_>>(),
    );
    let mut steps = order.iter().map(|(at, _)| (lines[*at].1, lines[*at].3));
    if !matches!(steps.next(), Some((_, Step::Opened))) {
        return Err(NotAConversation::Unopened(id.clone()));
    }
    // Each turn's instant and whether it opens a round, the spans set aside, and how many proposals.
    let mut turns: Vec<(Datetime, bool)> = Vec::new();
    let mut aside: Vec<std::ops::Range<usize>> = Vec::new();
    let mut proposals = 0;
    for (at, step) in steps {
        match step {
            Step::Asks => turns.push((at, true)),
            Step::Answers => turns.push((at, false)),
            Step::Rewound { to } if to > turns.len() => {
                return Err(NotAConversation::Unsaid { id: id.clone(), to });
            }
            Step::Rewound { to } => aside.push(to..turns.len()),
            Step::Proposed => proposals += 1,
            Step::Coding { proposal } if proposal >= proposals => {
                return Err(NotAConversation::Phantom {
                    id: id.clone(),
                    index: proposal,
                });
            }
            Step::Opened | Step::Still | Step::Coding { .. } => {}
        }
    }
    let live = turns
        .iter()
        .enumerate()
        .filter(|(turn, _)| !aside.iter().any(|span| span.contains(turn)));
    let mut openings = Vec::new();
    for (count, (turn, &(at, opens))) in live.enumerate() {
        if count == 0 || opens {
            openings.push(Opening { turn, at });
        }
    }
    Ok(openings)
}

/// The outline of a conversation read whole.
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
