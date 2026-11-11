//! Reading the corpus as the app's pages read it: a paged inbox, a paged search, one message, one
//! conversation.

use super::Datetime;

/// A notmuch query; an empty one is a mistake, not a search for everything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query(String);

/// The longest query the corpus will be asked.
pub const MAX_QUERY_CHARS: usize = 500;

impl Query {
    /// A query, or the reason this text is not one.
    pub fn new(text: impl Into<String>) -> Result<Self, MailError> {
        let text = text.into();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(MailError::Query(
                "a query is required: plain words, from:, subject:, date: …".into(),
            ));
        }
        if trimmed.chars().count() > MAX_QUERY_CHARS {
            return Err(MailError::Query(format!(
                "a query is capped at {MAX_QUERY_CHARS} characters, got {}",
                trimmed.chars().count()
            )));
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The query as the index reads it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The corpus refused a read.
#[derive(Debug, thiserror::Error)]
pub enum MailError {
    /// The text handed in is not a query.
    #[error("not a query: {0}")]
    Query(String),
    /// The index could not be read.
    #[error("the mail index could not be read: {0}")]
    Unreachable(String),
    /// The corpus holds no such message or conversation, which is not an empty result.
    #[error("the local corpus holds no {kind} {name:?}")]
    Absent {
        /// What was looked for: "message" or "conversation".
        kind: &'static str,
        /// The identifier that found nothing.
        name: String,
    },
}

/// Reading the corpus, and only reading it; sending is a separate trait nothing the agent holds.
///
/// Every list takes a cap and a cursor because loading a message hashes every file it matches.
pub trait Mailbox: Sync {
    /// Every account there is; a filter naming one absent from this is a refusal, never an empty
    /// inbox.
    fn accounts(&self) -> Vec<String>;

    /// The most recent INBOX mail by date, whatever triage made of it.
    fn inbox(
        &self,
        account: Option<&str>,
        limit: usize,
        before: Option<Datetime>,
    ) -> Result<Vec<crate::mail::MailRow>, MailError>;

    /// Any query, newest first, paged by `before` exactly as the inbox is.
    fn search(
        &self,
        query: &Query,
        account: Option<&str>,
        limit: usize,
        before: Option<Datetime>,
    ) -> Result<Vec<crate::mail::MailRow>, MailError>;

    /// One message by notmuch id or by canonical hash (full or a unique prefix).
    fn message(&self, id: &str) -> Result<crate::mail::MailMessage, MailError>;

    /// One conversation, oldest first, the newest `limit` of it when it is longer.
    fn thread(
        &self,
        thread: &str,
        limit: usize,
    ) -> Result<Vec<crate::mail::MailMessage>, MailError>;

    /// Every attachment on one message, `id` resolved as [`Mailbox::message`] resolves it.
    fn attachments(&self, id: &str) -> Result<Vec<crate::mail::AttachmentMeta>, MailError>;

    /// One message file, by its notmuch id, recorded for a forward and never its bytes, which
    /// only the owner's tap reads ([`crate::held_value::Reveal`]); a file over `cap` bytes is
    /// not read.
    fn stored(&self, id: &str, cap: u64) -> Result<crate::mail::Stored, MailError>;

    /// One attachment's bytes; an unlisted `name` is [`MailError::Absent`].
    fn attachment(&self, id: &str, name: &str) -> Result<crate::mail::AttachmentFile, MailError>;

    /// One attachment, read for the model: the file itself, its text, or why it is neither;
    /// an unlisted `name` is [`MailError::Absent`].
    fn read_attachment(
        &self,
        id: &str,
        name: &str,
    ) -> Result<crate::file_read::FileRead, MailError>;
}
