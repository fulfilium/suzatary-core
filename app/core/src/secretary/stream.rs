//! Every conversation's live rounds as one stream, ordered by the instant of the turn that opened
//! each, filtered by topics: what the secretary page draws (`docs/TOPICS-PLAN-2026-10-21.md` §6).
//!
//! The rounds are chosen from the conversations' outlines and only then read: [`choose`] needs no
//! word, and [`stream`] reads only the conversations of the rounds it draws.

use std::collections::{BTreeMap, BTreeSet};

use crate::capability::Turn;
use crate::secretary::outline::{Opening, Outline};
use crate::secretary::readings::{Dress, Place, turn};
use crate::secretary::store::{ConversationId, NotAConversation, Said, Stored};
use crate::secretary::topics::reading::Reading;
use crate::secretary::topics::target::Target;
use crate::secretary::topics::targets::{MAX_TOPICS, Round, rounds};
use crate::secretary::topics::topic::Topic;
use crate::wire;

/// How many rounds one page of the stream holds.
pub const PAGE: usize = 30;

/// The rounds a page of the stream draws, oldest first, each by its conversation and its opening,
/// and how many rounds the filter shows in all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice<'o> {
    /// The rounds drawn.
    pub drawn: Vec<(&'o ConversationId, Opening)>,
    /// How many the filter shows.
    pub total: usize,
}

impl Choice<'_> {
    /// The conversations the drawn rounds are in.
    #[must_use]
    pub fn conversations(&self) -> BTreeSet<&ConversationId> {
        self.drawn.iter().map(|(id, _)| *id).collect()
    }
}

/// Why the stream does not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unread {
    /// A conversation's lines are none, the first such by id, as the whole store's reading says.
    #[error(transparent)]
    Unfolded(#[from] NotAConversation),
    /// A drawn round's conversation was not read.
    #[error("rounds of {0} were chosen and its lines were not read")]
    Undrawn(ConversationId),
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
        for opening in outline.as_ref()? {
            let target = Target::Round {
                conversation: id.clone(),
                turn: opening.turn,
            };
            if topics.shows(chosen, &target) {
                shown.push((opening.at, id, opening.turn));
            }
        }
    }
    shown.sort();
    let total = shown.len();
    Ok(Choice {
        drawn: shown
            .into_iter()
            .skip(total.saturating_sub(pages.saturating_mul(PAGE)))
            .map(|(at, id, turn)| (id, Opening { turn, at }))
            .collect(),
        total,
    })
}

/// The newest `pages` pages of the rounds of `outlines` that carry every topic of `chosen`, oldest
/// first, and how many rounds the filter shows in all: each drawn round read from its
/// conversation as `read` gives it (`None` where it holds no line of it), and no other
/// conversation read. A round is drawn where the conversation's reading opens one at the same
/// turn and instant; one its reading left out is not.
///
/// # Errors
/// [`Unread::Unfolded`] for an outline, or a drawn conversation, whose lines are no conversation;
/// [`Unread::Undrawn`] for a drawn round's conversation that `read` does not give.
pub fn stream<'o, S: std::borrow::Borrow<Stored>>(
    outlines: impl IntoIterator<Item = (&'o ConversationId, &'o Outline)>,
    mut read: impl FnMut(&ConversationId) -> Option<Result<S, NotAConversation>>,
    topics: &Reading,
    chosen: &BTreeSet<Topic>,
    pages: usize,
    dress: &Dress<'_>,
) -> Result<wire::StreamPayload, Unread> {
    let choice =
        choose(outlines, topics, chosen, pages).map_err(|e| Unread::Unfolded(e.clone()))?;
    let read: BTreeMap<&ConversationId, S> = choice
        .conversations()
        .into_iter()
        .filter_map(|id| Some(read(id)?.map(|stored| (id, stored))))
        .collect::<Result<_, _>>()?;
    let mut drawn = Vec::new();
    for (id, opening) in &choice.drawn {
        let stored = read
            .get(id)
            .ok_or_else(|| Unread::Undrawn((*id).clone()))?
            .borrow();
        // A round whose lines the reading left out (a print that does not decode) is not drawn,
        // and every round it still reads is.
        if let Some(round) = rounds(stored)
            .into_iter()
            .find(|round| round.opening.0 == opening.turn && round.opening.1.at == opening.at)
        {
            drawn.push(row(stored, &round, topics, dress));
        }
    }
    Ok(wire::StreamPayload {
        rounds: drawn,
        total: choice.total,
    })
}

fn row(
    stored: &Stored,
    round: &Round<'_>,
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
    let on = topics.topics(&target);
    wire::StreamRound {
        target: target.to_string(),
        conversation: stored.id.as_str().to_owned(),
        title: stored.heading().into_owned(),
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
