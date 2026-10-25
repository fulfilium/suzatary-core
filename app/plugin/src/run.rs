//! What one call of a plugin is lent, and what it did.

use std::sync::Arc;

use suzatary_core::plugin::{CredentialName, Refusal, Said};
use suzatary_io::http::Http;
use suzatary_io::secrets::Resolves;

/// What one call reaches. A plugin holds none of these: the host answers its imports with them.
///
/// `http` bounds every plugin's reach: the deployment passes
/// `suzatary_io::http::Ureq::public_only()`, which follows no redirect and refuses any name
/// resolving off the public internet, so no manifest can aim a plugin at this box or the tailnet.
#[derive(Clone)]
pub struct Effects {
    /// The one way a request leaves.
    pub http: Arc<dyn Http>,
    /// The secret store, asked by name only for a credential the manifest declares.
    pub secrets: Arc<dyn Resolves + Send + Sync>,
}

/// What a source returned: its items, unparsed, and the cursor the next call starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synced {
    /// Each item's bytes, in the order the plugin gave them.
    pub items: Vec<Vec<u8>>,
    /// Opaque to the host.
    pub cursor: Vec<u8>,
}

/// Why a call produced nothing. Whatever it repeats of the plugin's words is [`Said`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    /// The plugin answered with an error of its own.
    #[error("the plugin failed: {0}")]
    Plugin(Said),
    /// The component could not be instantiated in a fresh store.
    #[error("the plugin could not be instantiated: {0}")]
    Instantiate(Said),
    /// The call outlived its budget's time and was interrupted.
    #[error("the plugin ran past its time budget and was stopped")]
    Deadline,
    /// The plugin grew past its budget's memory and was stopped.
    #[error("the plugin grew past its memory budget and was stopped")]
    Memory,
    /// The plugin trapped: a panic, an unreachable, a stack overflow, a flood of its log.
    #[error("the plugin trapped: {0}")]
    Trapped(Said),
}

/// One fetch, as the host saw it, so a reader never rests on what a plugin says of itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    /// The declared credential it named as its URL; `None` for a URL the plugin wrote.
    pub credential: Option<CredentialName>,
    /// The status it was answered with, or why it was not sent or not handed back.
    pub outcome: Result<u16, Refusal>,
}

/// One call, as the host saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// The answer.
    pub result: Result<Synced, RunError>,
    /// Every fetch asked for, in order, refused or answered.
    pub fetches: Vec<Fetch>,
    /// The lines the plugin wrote to its standard error, each bounded, at most a hundred.
    pub said: Vec<Said>,
}
