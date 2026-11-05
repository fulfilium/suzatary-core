//! One proposal line against the inbox's reading: placed, or held with why not.

use super::{Asking, Standing};
use serde::Serialize;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "camelCase")]
pub enum Unheld {
    /// On a replica, a record not taken in yet; on the box, which files the record before the
    /// line, a damaged object the store's read leaves out.
    #[error("the inbox holds no record of it")]
    NotHeld,
    /// Responses to it and no asking.
    #[error("it is answered and was never asked")]
    Unasked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unplaced {
    pub asking: Asking,
    pub why: Unheld,
}

/// Placed with its object, or held as a named failure, as a chain set holds a member that does
/// not read.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Placed(Box<Standing>),
    Unplaced(Unplaced),
}

impl Line {
    #[must_use]
    pub fn asking(&self) -> &Asking {
        match self {
            Line::Placed(standing) => &standing.asking,
            Line::Unplaced(unplaced) => &unplaced.asking,
        }
    }
}
