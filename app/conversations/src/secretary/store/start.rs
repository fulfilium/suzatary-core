//! What begins a conversation.

use suzatary_base::capability::Datetime;
use suzatary_memory::secretary::words::Words;

use super::Kind;

/// What begins a conversation, read from its first line by
/// [`Entry::start`](super::Entry::start) alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Start<'a> {
    pub at: Datetime,
    pub title: &'a Words<String>,
    pub kind: Kind,
    pub begun: Begun<'a>,
}

/// The line a conversation begins with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Begun<'a> {
    Opened {
        /// The model's own name for itself.
        model: &'a str,
    },
    /// Set up from a template, whose opening the box notes.
    SetUp(suzatary_connect::connect::TemplateId),
}
