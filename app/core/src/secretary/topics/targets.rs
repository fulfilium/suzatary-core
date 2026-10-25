//! The rounds and notes of one conversation, as targets a topic is put on.

use crate::secretary::SecretaryVerb;
use crate::secretary::outcome::Outcome;
use crate::secretary::store::{ConversationId, Said, Stored};
use crate::secretary::topics::target::Target;
use crate::secretary::turn_shape::{TurnShape, shape};

/// The most topics one round or note holds.
pub const MAX_TOPICS: usize = 3;

/// One live round: the turn that opened it and the turns after it, with their indices, oldest
/// first.
#[derive(Debug, Clone)]
pub struct Round<'a> {
    pub conversation: &'a ConversationId,
    pub opening: (usize, &'a Said),
    pub rest: Vec<(usize, &'a Said)>,
}

/// The live rounds, each opened by a person's or act's turn (or the first live turn), each
/// holding the turns up to the next round's.
#[must_use]
pub fn rounds(stored: &Stored) -> Vec<Round<'_>> {
    let mut rounds: Vec<Round<'_>> = Vec::new();
    for (turn, said) in stored.live() {
        let opens = matches!(
            shape(&said.turn),
            TurnShape::Person { .. } | TurnShape::Act { .. }
        );
        match rounds.last_mut() {
            Some(round) if !opens => round.rest.push((turn, said)),
            Some(_) | None => rounds.push(Round {
                conversation: &stored.id,
                opening: (turn, said),
                rest: Vec::new(),
            }),
        }
    }
    rounds
}

impl Round<'_> {
    #[must_use]
    pub fn target(&self) -> Target {
        Target::Round {
            conversation: self.conversation.clone(),
            turn: self.opening.0,
        }
    }

    /// Every turn of the round, the opening first.
    #[must_use]
    pub fn turns(&self) -> impl DoubleEndedIterator<Item = (usize, &Said)> + '_ {
        std::iter::once(self.opening).chain(self.rest.iter().copied())
    }

    /// Its notes.
    #[must_use]
    pub fn notes(&self) -> Vec<Target> {
        notes_in(self.conversation, &self.turns().collect::<Vec<_>>())
    }
}

/// The notes among `turns` of `conversation`: a `draft_note` call writes the note and then its
/// own answer, both answered turns of that verb, so a note is the first of each such pair.
#[must_use]
pub fn notes_in(conversation: &ConversationId, turns: &[(usize, &Said)]) -> Vec<Target> {
    let drafted = |said: &Said| {
        matches!(shape(&said.turn), TurnShape::Tool { verb, outcome: Outcome::Answered, .. }
            if verb == SecretaryVerb::DraftNote.into())
    };
    let mut notes = Vec::new();
    let mut at = 0;
    while at < turns.len() {
        let (turn, said) = turns[at];
        if drafted(said) && turns.get(at + 1).is_some_and(|(_, next)| drafted(next)) {
            notes.push(Target::Note {
                conversation: conversation.clone(),
                turn,
            });
            at += 2;
        } else {
            at += 1;
        }
    }
    notes
}
