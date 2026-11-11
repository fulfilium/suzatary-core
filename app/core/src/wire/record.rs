//! The record's wire.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubTodo {
    /// Typst markup.
    pub body: String,
    pub done: bool,
}
