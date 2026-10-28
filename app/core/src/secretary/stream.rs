//! Every conversation's live rounds as one stream, ordered by the instant of the turn that opened
//! each, filtered by topics: what the secretary page draws (`docs/TOPICS-PLAN-2026-10-21.md` §6).
//!
//! The rounds are chosen from the conversations' outlines and only then read: [`choose`] needs no
//! word, and [`stream`] reads only the lines the outline names for each drawn round: its title,
//! its turns, and the reseals and erasures of their slots. A reader holding every line and one
//! holding only those draw the same rows, so the eager and the lazy stream are this one function.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;

use crate::capability::Turn;
use crate::secretary::outline::{Outline, Round, Rounds};
use crate::secretary::readings::{Dress, Place, turn};
use crate::secretary::store::{ConversationId, Entry, NotAConversation, Said, heading};
use crate::secretary::topics::reading::Reading;
use crate::secretary::topics::target::Target;
use crate::secretary::topics::targets::{self, MAX_TOPICS};
use crate::secretary::topics::topic::Topic;
use crate::secretary::words::Words;
use crate::wire;

/// How many rounds one page of the stream holds.
pub const PAGE: usize = 30;

/// One round a page draws: its conversation, that conversation's outline, and the round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drawn<'o> {
    /// The conversation.
    pub conversation: &'o ConversationId,
    /// Its outline.
    pub rounds: &'o Rounds,
    /// The round.
    pub round: &'o Round,
}

impl Drawn<'_> {
    /// Every line whose words the round reads.
    #[must_use]
    pub fn objects(&self) -> BTreeSet<Hash> {
        self.rounds.objects(self.round)
    }
}

/// The rounds a page of the stream draws, oldest first, and how many rounds the filter shows in
/// all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice<'o> {
    /// The rounds drawn.
    pub drawn: Vec<Drawn<'o>>,
    /// How many the filter shows.
    pub total: usize,
}

impl Choice<'_> {
    /// Every line whose words the drawn rounds read: all a reader must hold to draw them.
    #[must_use]
    pub fn objects(&self) -> BTreeSet<Hash> {
        self.drawn.iter().flat_map(Drawn::objects).collect()
    }
}

/// What a reader holds of the lines a round reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held<L> {
    /// Every line, each opened as its conversation opens it, by name.
    Opened(L),
    /// A print that is no line of the round's conversation: the round is not drawn.
    Unread,
    /// The first line, by name, the reader does not hold.
    Lacking(Hash),
}

/// Why the stream does not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unread {
    /// A conversation's lines are none, the first such by id, as the whole store's reading says.
    #[error(transparent)]
    Unfolded(#[from] NotAConversation),
    /// A drawn round's line was not given.
    #[error("a round of {conversation} was chosen and its line {line} was not read")]
    Undrawn {
        /// The round's conversation.
        conversation: ConversationId,
        /// The line.
        line: Hash,
    },
}

/// The newest `pages` pages of the rounds of `outlines` that carry every topic of `chosen`.
///
/// # Errors
/// The first outline, by id, whose lines are no conversation.
pub fn choose<'o>(
    outlines: impl IntoIterator<Item = (&'o ConversationId, &'o Outline)>,
    topics: &Reading,
    chosen: &BTreeSet<Topic>,
    pages: usize,
) -> Result<Choice<'o>, &'o NotAConversation> {
    let mut by: BTreeMap<&ConversationId, &Outline> = BTreeMap::new();
    by.extend(outlines);
    let mut shown = Vec::new();
    for (id, outline) in by {
        let rounds = outline.as_ref()?;
        for (at, round) in rounds.rounds().iter().enumerate() {
            let target = Target::Round {
                conversation: id.clone(),
                turn: round.opening.turn,
            };
            if topics.shows(chosen, &target) {
                shown.push((round.opening.at, id, round.opening.turn, rounds, at));
            }
        }
    }
    shown.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
    let total = shown.len();
    Ok(Choice {
        drawn: shown
            .into_iter()
            .skip(total.saturating_sub(pages.saturating_mul(PAGE)))
            .map(|(_, conversation, _, rounds, at)| Drawn {
                conversation,
                rounds,
                round: &rounds.rounds()[at],
            })
            .collect(),
        total,
    })
}

/// The newest `pages` pages of the rounds of `outlines` that carry every topic of `chosen`, oldest
/// first, and how many rounds the filter shows in all: each drawn round from the lines `read`
/// gives for its conversation, its opening turn and its objects, and no other line read. A round
/// whose lines do not read as its outline names them is not drawn.
///
/// # Errors
/// [`Unread::Unfolded`] for an outline whose lines are no conversation; [`Unread::Undrawn`] for a
/// line of a drawn round that `read` does not hold.
pub fn stream<'o, L: std::borrow::Borrow<BTreeMap<Hash, Entry>>>(
    outlines: impl IntoIterator<Item = (&'o ConversationId, &'o Outline)>,
    mut read: impl FnMut(&ConversationId, usize, &BTreeSet<Hash>) -> Held<L>,
    topics: &Reading,
    chosen: &BTreeSet<Topic>,
    pages: usize,
    dress: &Dress<'_>,
) -> Result<wire::StreamPayload, Unread> {
    let choice =
        choose(outlines, topics, chosen, pages).map_err(|e| Unread::Unfolded(e.clone()))?;
    let mut drawn = Vec::new();
    for round in &choice.drawn {
        let lines = match read(
            round.conversation,
            round.round.opening.turn,
            &round.objects(),
        ) {
            Held::Opened(lines) => lines,
            Held::Unread => continue,
            Held::Lacking(line) => {
                return Err(Unread::Undrawn {
                    conversation: round.conversation.clone(),
                    line,
                });
            }
        };
        if let Some(row) = opened(round, lines.borrow(), topics, dress) {
            drawn.push(row);
        }
    }
    Ok(wire::StreamPayload {
        rounds: drawn,
        total: choice.total,
    })
}

/// The reader that holds every line of a store, each opened as its conversation opens it
/// ([`crate::secretary::transcript::opened`]): the eager stream, whose rows the lazy one must draw.
pub fn every(
    opened: &BTreeMap<Hash, Entry>,
) -> impl FnMut(&ConversationId, usize, &BTreeSet<Hash>) -> Held<BTreeMap<Hash, Entry>> + '_ {
    |_, _, objects| {
        let mut lines = BTreeMap::new();
        for name in objects {
            match opened.get(name) {
                Some(entry) => lines.insert(name.clone(), entry.clone()),
                None => return Held::Lacking(name.clone()),
            };
        }
        Held::Opened(lines)
    }
}

/// The row of `round` from its lines opened: its title from the title line's entry and each turn
/// from its line's, as the conversation's fold reads them; `None` where a named line says no title
/// or no turn.
fn opened(
    round: &Drawn<'_>,
    lines: &BTreeMap<Hash, Entry>,
    topics: &Reading,
    dress: &Dress<'_>,
) -> Option<wire::StreamRound> {
    let title = lines.get(round.rounds.title())?.title()?;
    let turns: Vec<(usize, Said)> = round
        .round
        .turns
        .iter()
        .map(|(index, line)| Some((*index, Said::of(lines.get(line)?)?)))
        .collect::<Option<_>>()?;
    let (opening, rest) = turns.split_first()?;
    let round = targets::Round {
        conversation: round.conversation,
        opening: (opening.0, &opening.1),
        rest: rest.iter().map(|(index, said)| (*index, said)).collect(),
    };
    Some(row(&title, &round, topics, dress))
}

/// The row a stream draws for `round` of a conversation titled `title`.
#[must_use]
pub fn row(
    title: &Words<String>,
    round: &targets::Round<'_>,
    topics: &Reading,
    dress: &Dress<'_>,
) -> wire::StreamRound {
    let drawn = |(index, said): (usize, &Said)| {
        turn(
            dress.answers,
            said,
            Place {
                index,
                queued: false,
                attempt: wire::Attempt::Current,
            },
            dress.memory,
        )
    };
    let target = round.target();
    let on = topics.tags(&target);
    wire::StreamRound {
        target: target.to_string(),
        conversation: round.conversation.as_str().to_owned(),
        title: heading(title).into_owned(),
        turn: round.opening.0,
        opening: drawn(round.opening),
        answer: round
            .turns()
            .rev()
            .find(|(_, said)| matches!(&said.turn, Turn::Model { text, .. } if !text.is_empty()))
            .map(drawn),
        room: MAX_TOPICS.saturating_sub(on.len()),
        topics: on.into_iter().map(String::from).collect(),
    }
}
