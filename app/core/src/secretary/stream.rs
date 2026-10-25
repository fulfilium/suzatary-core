//! Every conversation's live rounds as one stream, ordered by the instant of the turn that opened
//! each, filtered by topics: what the secretary page draws (`docs/TOPICS-PLAN-2026-10-21.md` §6).

use std::borrow::Borrow;
use std::collections::BTreeSet;

use crate::capability::Turn;
use crate::secretary::readings::{Dress, Place, turn};
use crate::secretary::store::{Said, Stored};
use crate::secretary::topics::reading::Reading;
use crate::secretary::topics::targets::{MAX_TOPICS, Round, rounds};
use crate::secretary::topics::topic::Topic;
use crate::wire;

/// How many rounds one page of the stream holds.
pub const PAGE: usize = 30;

/// The newest `pages` pages of the rounds of `stored` that carry every topic of `chosen`, oldest
/// first, and how many rounds the filter shows in all.
#[must_use]
pub fn stream<S: Borrow<Stored>>(
    stored: &[S],
    topics: &Reading,
    chosen: &BTreeSet<Topic>,
    pages: usize,
    dress: &Dress<'_>,
) -> wire::StreamPayload {
    let mut shown: Vec<(&Stored, Round<'_>)> = stored
        .iter()
        .map(Borrow::borrow)
        .flat_map(|stored| rounds(stored).into_iter().map(move |round| (stored, round)))
        .filter(|(_, round)| topics.shows(chosen, &round.target()))
        .collect();
    shown.sort_by(|(_, one), (_, two)| {
        one.opening
            .1
            .at
            .cmp(&two.opening.1.at)
            .then_with(|| one.conversation.cmp(two.conversation))
            .then_with(|| one.opening.0.cmp(&two.opening.0))
    });
    let total = shown.len();
    wire::StreamPayload {
        rounds: shown
            .iter()
            .skip(total.saturating_sub(pages.saturating_mul(PAGE)))
            .map(|(stored, round)| row(stored, round, topics, dress))
            .collect(),
        total,
    }
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
