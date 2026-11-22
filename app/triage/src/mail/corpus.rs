//! The mail corpus as the agent operates it; nothing here sends, and every answer's JSON is the
//! versioned contract the verbs print.

use crate::mail::heartbeat::PassOutcome;
use crate::mail::state::Decision;
use crate::mail::verbs::{Marked, Show, Summary};
use suzatary_mail::mail::mailbox::Query;
use suzatary_mail::mail::sync_report::SyncReport;

/// A corpus operation ran and refused; one variant, because every failure is text to read.
#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    /// Verbatim, so a refusal can teach the agent the right shape.
    #[error("{0}")]
    Refused(String),
}

/// The mail corpus as the agent operates it; the interpreter is `suzatary_io::corpus::Operator`.
pub trait Corpus: Sync {
    fn sync(&self) -> Result<SyncReport, CorpusError>;

    /// Inbox mail not yet seen by canonical hash, not noise.
    fn queue(&self, limit: Option<usize>) -> Result<Vec<Summary>, CorpusError>;

    /// Any notmuch query, newest first.
    fn search(&self, query: &Query, limit: Option<usize>) -> Result<Vec<Summary>, CorpusError>;

    /// One message in full, by notmuch id or by canonical hash or a prefix of one.
    fn show(&self, id: &str) -> Result<Show, CorpusError>;

    fn mark(
        &self,
        hash: &str,
        decision: Decision,
        note: Option<&str>,
        native: Option<&str>,
    ) -> Result<Marked, CorpusError>;

    /// `None` when nothing readable has been recorded yet.
    fn last_sync(&self) -> Option<SyncReport>;

    /// When the wrapper last reached its trusted push step, as RFC 3339, read from outside the
    /// chain so a pass that cannot commit still shows.
    fn last_triage_pass(&self) -> Option<String>;

    fn last_triage_outcome(&self) -> PassOutcome {
        PassOutcome::Absent
    }
}
