//! `[secretary] reserve`: the share of the Claude subscription's five-hour window the compactor
//! leaves for the owner.

use serde::{Deserialize, Serialize};

/// A share in `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Reserve(f64);

impl Reserve {
    /// Parse one.
    ///
    /// # Errors
    /// [`BadReserve`] outside `[0, 1]`, NaN included.
    pub fn parse(share: f64) -> Result<Self, BadReserve> {
        if (0.0..=1.0).contains(&share) {
            Ok(Self(share))
        } else {
            Err(BadReserve(share))
        }
    }

    /// The share.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Reserve {
    type Error = BadReserve;
    fn try_from(share: f64) -> Result<Self, BadReserve> {
        Self::parse(share)
    }
}

impl From<Reserve> for f64 {
    fn from(reserve: Reserve) -> Self {
        reserve.0
    }
}

impl Default for Reserve {
    /// 30% held back when the file names none.
    fn default() -> Self {
        Self(0.30)
    }
}

/// A reserve outside `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BadReserve(pub f64);

impl std::fmt::Display for BadReserve {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a reserve is a share between 0 and 1, got {}", self.0)
    }
}

impl std::error::Error for BadReserve {}
