//! Reading the Junk folder of a Microsoft account, for the monthly sweep.
//!
//! Read-only by construction: no method moves, flags or deletes, and the folder
//! is opened with `EXAMINE`.

use chrono::NaiveDate;

use crate::mail::junk::{ChurnList, JunkHeader, JunkMessage, Uid};
use crate::mail::oauth::NeedsLogin;

/// Why a Junk read produced nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JunkFailure {
    /// The account's IMAP grant is missing or lapsed; only a person can fix it.
    #[error("{0}")]
    NeedsLogin(NeedsLogin),
    /// Not one of the Microsoft accounts this box reads.
    #[error("{0} is not a Microsoft account on this deployment's roster")]
    Unknown(String),
    /// The server lists no folder marked `\Junk` and none by a Junk name.
    #[error("{0} has no Junk folder: nothing is marked \\Junk and nothing is named Junk")]
    NoJunkFolder(String),
    /// No message by that UID in the window's folder, now.
    #[error("{account}'s Junk holds no message with UID {uid}")]
    Missing {
        /// The account.
        account: String,
        /// The UID asked for.
        uid: u32,
    },
    /// The server could not be reached or refused, in its own words.
    #[error("{account}'s Junk could not be read: {why}")]
    Unreachable {
        /// The account.
        account: String,
        /// What went wrong.
        why: String,
    },
}

/// The Microsoft accounts' Junk folders, as the secretary may read them.
pub trait Junk: Sync {
    /// Every account whose Junk may be read, in the roster's order.
    fn junk_accounts(&self) -> Vec<String>;

    /// The churn domains this deployment declared, parsed once.
    fn churn(&self) -> ChurnList;

    /// Every message's headers in `account`'s Junk that arrived on or after
    /// `since`.
    ///
    /// # Errors
    /// [`JunkFailure`].
    fn junk_headers(&self, account: &str, since: NaiveDate)
    -> Result<Vec<JunkHeader>, JunkFailure>;

    /// One message from `account`'s Junk, its body as text.
    ///
    /// # Errors
    /// [`JunkFailure`].
    fn junk_message(&self, account: &str, uid: Uid) -> Result<JunkMessage, JunkFailure>;
}
