//! Why reading the mail corpus failed, beside the held values that read it.

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("not a query: {0}")]
    Query(String),
    #[error("the mail index could not be read: {0}")]
    Unreachable(String),
    /// The corpus holds no such message or conversation, which is not an empty result.
    #[error("the local corpus holds no {kind} {name:?}")]
    Absent {
        /// What was looked for: "message" or "conversation".
        kind: &'static str,
        name: String,
    },
}
