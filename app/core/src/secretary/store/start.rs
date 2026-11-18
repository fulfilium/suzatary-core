//! What begins a conversation.

use suzatary_base::capability::Datetime;
use suzatary_memory::secretary::words::Words;

use super::Kind;

/// What begins a conversation, read from its first line by
/// [`Entry::start`](super::Entry::start) alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Start<'a> {
    /// When.
    pub at: Datetime,
    /// Its title as begun.
    pub title: &'a Words<String>,
    /// What it is for.
    pub kind: Kind,
    /// The line that began it.
    pub begun: Begun<'a>,
}

/// The line a conversation begins with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Begun<'a> {
    /// Opened, behind the model that named itself so.
    Opened {
        /// The model's own name for itself.
        model: &'a str,
    },
    /// Set up from a template, whose opening the box notes.
    SetUp(suzatary_connect::connect::TemplateId),
}
