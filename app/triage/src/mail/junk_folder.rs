//! Reading a Microsoft account's Junk folder, read-only by construction: no method moves, flags
//! or deletes, and the folder is opened with `EXAMINE`.

use chrono::NaiveDate;

use crate::mail::junk::{ChurnList, JunkHeader, JunkMessage, Uid};
use suzatary_mail::mail::oauth::NeedsLogin;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JunkFailure {
    /// Only a person can fix it.
    #[error("{0}")]
    NeedsLogin(NeedsLogin),
    #[error("{0} is not a Microsoft account on this deployment's roster")]
    Unknown(String),
    #[error("{0} has no Junk folder: nothing is marked \\Junk and nothing is named Junk")]
    NoJunkFolder(String),
    #[error("{account}'s Junk holds no message with UID {uid}")]
    Missing {
        account: String,
        /// The UID asked for.
        uid: u32,
    },
    #[error("{account}'s Junk could not be read: {why}")]
    Unreachable {
        account: String,
        /// The server's own words.
        why: String,
    },
}

/// The Microsoft accounts' Junk folders, as the secretary may read them.
pub trait Junk: Sync {
    /// In the roster's order.
    fn junk_accounts(&self) -> Vec<String>;

    fn churn(&self) -> ChurnList;

    /// Every message that arrived on or after `since`.
    fn junk_headers(&self, account: &str, since: NaiveDate)
    -> Result<Vec<JunkHeader>, JunkFailure>;

    fn junk_message(&self, account: &str, uid: Uid) -> Result<JunkMessage, JunkFailure>;
}
