//! What this box has spent of its model subscription's allowance, readable only by `Server`.
//!
//! The reader passes numbers and nothing else: the transcripts quote mail and todos,
//! so no message text may reach a [`crate::usage::Request`].

use crate::usage::{Allowance, Refused, Request, Scope, Window};

/// The reader's failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UsageError {
    /// The transcripts directory could not be walked.
    #[error("the transcripts could not be read: {0}")]
    Unreadable(String),
}

/// What this box has spent, and what it was told about the allowance.
pub trait Usage: Sync {
    /// Every request the transcripts record, deduplicated by request id, in any order.
    ///
    /// # Errors
    /// The files being unreadable; an unparseable line is skipped, not raised.
    fn requests(&self) -> Result<Vec<Request>, UsageError>;

    /// Every refusal the limits recorded, with what each said about itself.
    ///
    /// # Errors
    /// As [`Usage::requests`].
    fn refusals(&self) -> Result<Vec<Refused>, UsageError>;

    /// Read everything the report will need now, so the first page view does not
    /// pay for the walk; answers how many files it read.
    ///
    /// # Errors
    /// The files being unreadable.
    fn warm_transcripts(&self) -> Result<usize, UsageError> {
        Ok(0)
    }

    /// How far back the reader looked, so the page can say `since <here>`; `None`
    /// means everything was read.
    fn horizon(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        None
    }

    /// What this box knows about the allowance per window; an absent window reads
    /// as [`Allowance::Unknown`].
    fn allowance(&self) -> Vec<(Window, Allowance)>;

    /// Which project directories this reader read, and which it found but did not.
    ///
    /// # Errors
    /// The root or one of its entries being unreadable, which is not an empty scope.
    fn scope(&self) -> Result<Scope, UsageError> {
        Ok(Scope::default())
    }
}
