//! The mail corpus as the agent operates it; nothing here sends. Every answer is a typed
//! record of [`crate::mail::verbs`], whose JSON is the versioned contract the verbs print.

use super::Query;
use crate::mail::state::Decision;
use crate::mail::sync_report::SyncReport;
use crate::mail::verbs::{Marked, Show, Summary};
use crate::secretary::heartbeat::PassOutcome;

/// A corpus operation ran and refused; one variant, because every failure is text to read.
#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    /// What went wrong, verbatim, so a refusal can teach the agent the right shape.
    #[error("{0}")]
    Refused(String),
}

/// The mail corpus as the agent operates it; the interpreter is `suzatary_io::corpus::Operator`.
pub trait Corpus: Sync {
    /// Pull every account per its strategy, reindex, re-tag the inbox.
    fn sync(&self) -> Result<SyncReport, CorpusError>;

    /// The triage queue: inbox mail not yet seen by canonical hash, not noise.
    fn queue(&self, limit: Option<usize>) -> Result<Vec<Summary>, CorpusError>;

    /// Any notmuch query, newest first.
    fn search(&self, query: &Query, limit: Option<usize>) -> Result<Vec<Summary>, CorpusError>;

    /// One message in full, by notmuch id or by canonical hash or a prefix of one.
    fn show(&self, id: &str) -> Result<Show, CorpusError>;

    /// Record a decision about a message, keyed by its canonical hash.
    fn mark(
        &self,
        hash: &str,
        decision: Decision,
        note: Option<&str>,
        native: Option<&str>,
    ) -> Result<Marked, CorpusError>;

    /// The most recent sync's report, `None` when nothing readable has been recorded yet.
    fn last_sync(&self) -> Option<SyncReport>;

    /// When the wrapper last reached its trusted push step, as RFC 3339; read from outside the
    /// chain so a pass that cannot commit still shows.
    fn last_triage_pass(&self) -> Option<String>;

    /// How the wrapper's last pass ended, which tells a pass failing every run apart from a
    /// healthy one.
    fn last_triage_outcome(&self) -> PassOutcome {
        PassOutcome::Absent
    }
}
