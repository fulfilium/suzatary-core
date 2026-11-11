//! Searching mail older than the routine sync's window on each account's own server.

use crate::mail::older::{AccountOlder, OlderQuery, Range};

/// The mail servers, searched read-only for what the routine sync never fetched; what matched
/// is kept on the box, already read, in a leaf of its own per account. The interpreter is
/// `suzatary_io::older::Servers`.
pub trait OlderMail: Sync {
    /// Search each of `accounts` on its server for `query` within `range`, pull at most
    /// [`crate::mail::older::MAX_MATCHES`] of what matched, index it, and answer each account's
    /// outcome in the order asked; one account's failure is its outcome, never the others'.
    fn search_older(
        &self,
        accounts: &[String],
        query: &OlderQuery,
        range: Range,
    ) -> Vec<AccountOlder>;
}
