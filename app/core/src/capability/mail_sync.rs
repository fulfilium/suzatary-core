//! The mail sync the timer runs, as an effect a verb may start early.

use super::Datetime;
use crate::mail::verbs::SyncReport;

/// A sync's report and when it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synced {
    /// When the report was written: the instant the sync finished.
    pub at: Datetime,
    /// What it found.
    pub report: SyncReport,
}

/// The sync could not be started; one variant, because every failure is text to read.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SyncFailure(pub String);

/// The timer's own sync, started on demand: the same unit and the same pull, so a sync outside
/// the timer reaches nothing the timer does not. The interpreter is `suzatary_io::mail_sync::Timer`.
pub trait MailSync: Sync {
    /// The last sync's report, `None` before the first and while one is being written.
    ///
    /// # Errors
    /// [`SyncFailure`] for a report that is there and cannot be read, which is not a missing one.
    fn last(&self) -> Result<Option<Synced>, SyncFailure>;

    /// Start the unit the timer starts, which never runs twice at once, and wait a bounded time
    /// for a report written after `since`; `None` when none came within it.
    ///
    /// # Errors
    /// [`SyncFailure`] when the unit could not be started.
    fn start(&self, since: Datetime) -> Result<Option<Synced>, SyncFailure>;
}
