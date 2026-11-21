//! The check a tap runs on a source it made readable.

use std::sync::Arc;

use suzatary_health::health::SourceKey;

/// Syncs nothing at a tap: a server with no plugin host, as a test's is unless it says.
pub(crate) struct Unsynced;

impl suzatary_connect::settings::Checks for Unsynced {
    fn check(&self, _: &SourceKey) {}
}

/// The checks a state's taps run.
pub(crate) type Checks = Arc<dyn suzatary_connect::settings::Checks + Send + Sync>;
