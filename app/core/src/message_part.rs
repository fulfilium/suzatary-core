//! The parts of a message that a held value points at.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The cap on an attachment's extracted characters, read for one fact rather than whole.
pub const MAX_ATTACHMENT_CHARS: usize = 100_000;

/// One header of a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MailHeader {
    /// The header's name.
    pub name: String,
    /// Its value.
    pub value: String,
}

/// A file to attach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// What the recipient sees it called.
    pub filename: String,
    /// Its bytes.
    pub content: Vec<u8>,
    /// Its full `type/subtype`.
    pub mime_type: String,
}
