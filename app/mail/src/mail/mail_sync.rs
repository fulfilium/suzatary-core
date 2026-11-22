//! The mail sync the timer runs, as an effect a verb may start early.

use super::Datetime;
use crate::mail::sync_report::SyncReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synced {
    /// The instant the sync finished.
    pub at: Datetime,
    pub report: SyncReport,
}

/// The sync could not be started; one variant, because every failure is text to read.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SyncFailure(pub String);

/// The timer's own sync, started on demand: the same unit and pull, so it reaches nothing the
/// timer does not.
pub trait MailSync: Sync {
    /// The last sync's report, `None` before the first and while one is being written.
    ///
    /// # Errors
    /// [`SyncFailure`] for a report that is there and cannot be read, which is not a missing one.
    fn last(&self) -> Result<Option<Synced>, SyncFailure>;

    /// Start the unit the timer starts, which never runs twice at once, and wait a bounded time
    /// for a report written after `since`; `None` when none came within it.
    fn start(&self, since: Datetime) -> Result<Option<Synced>, SyncFailure>;
}
