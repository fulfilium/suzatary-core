//! The compactor as a capability: a span of the conversations' log in, one line of words out.
//!
//! It answers text and nothing else, so a compactor cannot express a fact write, a chain
//! write or anything outward: the one writer of what it answered is
//! [`crate::secretary::memories::Held::summarize`], which reads the answer as words. An
//! interpreter sends [`crate::secretary::compact_ask::PROMPT`] as the whole system prompt and
//! [`Ask::text`] as it stands, with none of the secretary's framing.

use crate::capability::ModelError;
use crate::model_name::ModelName;
use crate::secretary::compact_ask::Ask;

/// A model asked to stand one line for a span.
pub trait Compact: Sync {
    /// The model that answers, as an attempt records it.
    fn model(&self) -> &ModelName;

    /// Its answer to `ask`, as said.
    ///
    /// # Errors
    /// [`ModelError`] when it could not be asked or did not answer.
    fn compact(&self, ask: &Ask) -> Result<String, ModelError>;
}
