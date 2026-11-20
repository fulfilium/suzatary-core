//! A conversation's answering state; the transcript reads it off the claims.

use serde::{Deserialize, Serialize};

/// A conversation's answering state, as a polling page reads it: a queued question, a running
/// round and a streaming reply are three facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum ConversationState {
    /// No round is open and nothing that wakes a model is owed: every request
    /// a round covered was closed, and what is left (the box's act lines)
    /// rides along with the next round.
    Idle,
    /// No round is open and a turn that wakes a model (a person's, or the
    /// owner's act) is owed, which no round covers yet.
    Queued,
    /// A round is open: claimed and not yet closed.
    Thinking,
    /// A partial reply is streaming. The transcript never reads this, since a partial is never written
    /// to the transcript; it is for the layer holding the live broadcast.
    Answering,
}
