//! Whether the compactor runs, and with what.

use crate::model_name::ModelName;
use crate::secretary::compact_budget::Budget;
use suzatary_conversations::nonempty::NonEmpty;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compacting {
    /// The kill switch: nothing is compacted, and the lines wait.
    Off,
    On {
        /// The models the compactor runs, in the order a node is owed to them.
        ladder: NonEmpty<ModelName>,
        /// What it may spend, all workers together.
        budget: Budget,
        /// How many steps may run at once, each on a node of its own.
        workers: std::num::NonZeroUsize,
    },
}
