//! Mail-in as a port: one account's mail pulled from its server into its maildir.

use super::fault::Fault;

/// Every message lands once, as the server holds it, in a leaf's `new/` or `cur/`
/// (`conformance::puller`).
pub trait Puller {
    /// Pull the account's mail, its INBOX from `since` where the kind searches by date.
    fn pull(&self, since: chrono::NaiveDate) -> Result<String, Unpulled>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{said}")]
pub struct Unpulled {
    /// The class, which names the fix card.
    pub class: Fault,
    pub said: String,
}

impl From<super::imap::ImapError> for Unpulled {
    fn from(failed: super::imap::ImapError) -> Self {
        Self {
            class: failed.class(),
            said: failed.to_string(),
        }
    }
}
