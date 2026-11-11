//! The base's wire.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Every refusal: the server's own sentence, shown verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ErrorPayload {
    pub error: String,
}
