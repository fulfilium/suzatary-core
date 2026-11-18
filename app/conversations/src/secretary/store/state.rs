//! A conversation's answering state, derived from its last live turn.

use super::Stored;
use crate::secretary::turn_shape::TurnShape;
use serde::{Deserialize, Serialize};

/// A conversation's answering state, as a polling page reads it: a queued question, a running
/// round and a streaming reply are three facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum ConversationState {
    /// Nothing is owed: the last turn is a model turn with no calls, an act,
    /// or there is no turn at all.
    Idle,
    /// A person's turn is the last one and nothing has answered it yet.
    Queued,
    /// A round is running: the last turn is a verb's answer, or a model turn
    /// that asked for calls still being run.
    Thinking,
    /// A partial reply is streaming. [`state`] never returns this, since a partial is never written
    /// to the transcript; it is for the layer holding the live broadcast.
    Answering,
}

/// Derive [`ConversationState`] from the last live turn, and nothing else.
///
/// A flag kept beside the transcript could be left set by a process that died, leaving a phone
/// polling forever. Live, because a regenerate leaves the question last: `Queued` is what asking
/// again means.
#[must_use]
pub fn state(stored: &Stored) -> ConversationState {
    match stored
        .live()
        .last()
        .map(|(_, said)| crate::secretary::turn_shape::shape(&said.turn))
    {
        Some(TurnShape::Model { calls }) => {
            if calls.is_empty() {
                ConversationState::Idle
            } else {
                ConversationState::Thinking
            }
        }
        Some(TurnShape::Tool { .. }) => ConversationState::Thinking,
        Some(TurnShape::Person { .. }) => ConversationState::Queued,
        // An act starts no round, and neither does an empty transcript.
        None | Some(TurnShape::Act { .. }) => ConversationState::Idle,
    }
}
