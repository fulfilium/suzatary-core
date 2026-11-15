//! Why reading the mail corpus failed, beside the held values that read it.

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
