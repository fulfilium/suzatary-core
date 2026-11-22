use crate::mail::older::{AccountOlder, OlderQuery, Range};

/// The mail servers, searched read-only for what the routine sync never fetched; what matched is
/// kept on the box, already read, in a leaf of its own per account.
pub trait OlderMail: Sync {
    /// Each account's outcome in the order asked; one account's failure is its outcome, never the
    /// others'.
    fn search_older(
        &self,
        accounts: &[String],
        query: &OlderQuery,
        range: Range,
    ) -> Vec<AccountOlder>;
}
