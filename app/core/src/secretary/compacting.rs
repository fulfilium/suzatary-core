//! Whether the compactor runs, and with what: a sum, so a compactor that is off names no model
//! and no budget, and a running one names both.

use crate::model_name::ModelName;
use crate::nonempty::NonEmpty;
use crate::secretary::compact_budget::Budget;

/// The compactor as the instance file sets it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compacting {
    /// The kill switch: nothing is compacted, and the lines wait.
    Off,
    /// Compact with the models of `ladder`, within `budget`, `workers` steps at once.
    On {
        /// The models the compactor runs, in the order a node is owed to them.
        ladder: NonEmpty<ModelName>,
        /// What it may spend, all workers together.
        budget: Budget,
        /// How many steps may run at once, each on a node of its own.
        workers: std::num::NonZeroUsize,
    },
}
