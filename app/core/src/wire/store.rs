//! The store's wire.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A `since` the box does not hold is answered as a replica holding nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum SinceRead {
    Known,
    Unknown,
}
