//! The runs that claim rounds: a run nothing holds is gone, so a reader of an unclosed round
//! knows whether it may claim it again.

use suzatary_conversations::secretary::store::RunId;
use suzatary_store::store::StoreError;

/// Where runs are drawn and whether one is gone.
pub trait Runs: Sync {
    /// A new run, held until the answer drops.
    fn begin(&self) -> Result<Held, StoreError>;

    /// Whether nothing holds `run`: its round ended, or its process did.
    fn gone(&self, run: &RunId) -> bool;
}

/// A run held: gone once this drops, panic or not.
pub struct Held {
    pub run: RunId,
    _hold: Box<dyn std::any::Any + Send>,
}

impl Held {
    /// `run`, held while `hold` lives.
    #[must_use]
    pub fn new(run: RunId, hold: impl std::any::Any + Send) -> Self {
        Self {
            run,
            _hold: Box::new(hold),
        }
    }
}

impl std::fmt::Debug for Held {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Held({})", self.run)
    }
}
