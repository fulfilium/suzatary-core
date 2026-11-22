//! The parts of a message that a held value points at.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The cap on an attachment's extracted characters, read for one fact rather than whole.
pub const MAX_ATTACHMENT_CHARS: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MailHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub filename: String,
    pub content: Vec<u8>,
    /// Its full `type/subtype`.
    pub mime_type: String,
}
