//! Conformance suites for the app's ports: each suite checks an implementation against cases the
//! caller supplies and returns the first law broken.

pub mod drive;
pub mod puller;
pub mod transport;

use std::fmt;

/// A law an implementation broke, and what showed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broken<L> {
    pub law: L,
    pub detail: String,
}

impl<L> Broken<L> {
    pub fn of(law: L, detail: impl Into<String>) -> Self {
        Self {
            law,
            detail: detail.into(),
        }
    }
}

impl<L: fmt::Display> fmt::Display for Broken<L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the law that {} does not hold: {}",
            self.law, self.detail
        )
    }
}

impl<L: fmt::Debug + fmt::Display> std::error::Error for Broken<L> {}

pub fn hold<L>(law: L, holds: bool, detail: impl FnOnce() -> String) -> Result<(), Broken<L>> {
    if holds {
        Ok(())
    } else {
        Err(Broken::of(law, detail()))
    }
}
