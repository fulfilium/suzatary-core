//! The lines a resumed model session's bookkeeping wrote. A stored line rests on the line before
//! it, so these stay in every store that holds them; nothing writes them now, no reader folds
//! them into a conversation, and no page draws them.

/// Which retired line this is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "retired", rename_all = "lowercase")]
pub enum Retired {
    /// The conversation had been told the memory as of some version.
    MemorySeen,
    /// A model session was opened behind the conversation.
    Session,
    /// `model` said a turn the session never heard.
    Unheard { model: String },
}
