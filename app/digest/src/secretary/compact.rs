//! The compactor as a capability: a span of the conversations' log in, one line of words out.
//!
//! It answers text and nothing else, so a compactor cannot express a fact write, a chain
//! write or anything outward. An interpreter sends [`crate::secretary::compact_ask::PROMPT`] as
//! the whole system prompt and [`Ask::text`] as it stands, with none of the secretary's framing.

use crate::model_name::ModelName;
use crate::secretary::compact_ask::Ask;
use suzatary_conversations::secretary::model::ModelError;

/// A model asked to stand one line for a span.
pub trait Compact: Sync {
    fn model(&self) -> &ModelName;

    fn compact(&self, ask: &Ask) -> Result<String, ModelError>;
}
