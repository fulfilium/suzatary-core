//! Conformance suites for the app's ports: each port a trait, its laws a closed `Law`, and the
//! suite a function of an implementation and some cases that answers the first law broken.
//!
//! The suites are the library's, not a test's, so a second implementation proves itself by
//! calling them; the cases are the caller's (the tests generate them), so the suite is a function
//! of what it is given and never of a generator it hides.

pub mod drive;
pub mod puller;
pub mod transport;

use std::fmt;

/// A law an implementation broke: which, and what showed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broken<L> {
    /// The law.
    pub law: L,
    /// What showed it.
    pub detail: String,
}

impl<L> Broken<L> {
    /// `law` broken, as `detail` showed.
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

/// `holds` or the law it breaks, as `detail` says.
pub(crate) fn hold<L>(
    law: L,
    holds: bool,
    detail: impl FnOnce() -> String,
) -> Result<(), Broken<L>> {
    if holds {
        Ok(())
    } else {
        Err(Broken::of(law, detail()))
    }
}
