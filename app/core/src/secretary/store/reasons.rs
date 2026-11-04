//! Why a proposal was made, and what it rests on.

use crate::reference::Reference;
use serde::{Deserialize, Serialize};

/// Why a proposal was made, and what it rests on; never on a wire.
///
/// [`Reasons::new`] (which `serde` also goes through) refuses an empty `why` or `sources`, so a
/// hand-edited line cannot produce a proposal with nothing behind it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "RawReasons", into = "RawReasons")]
pub struct Reasons {
    why: String,
    sources: Vec<Reference>,
}

/// The shape on a stored line, and the only thing [`Reasons`] parses from.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawReasons {
    why: String,
    sources: Vec<Reference>,
}

/// A proposal offered with nothing behind it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NoReasons {
    /// No sentence saying what the message is for.
    #[error("why must say, in a sentence or two, what this is for and what led to it")]
    Silent,
    /// No references.
    #[error(
        "sources must name at least one thing this rests on — a mail message, a thread, \
         a todo, a briefing or an earlier proposal"
    )]
    Groundless,
}

impl Reasons {
    /// Build one.
    ///
    /// # Errors
    /// [`NoReasons`] for an empty `why` or an empty `sources`.
    pub fn new(why: impl Into<String>, sources: Vec<Reference>) -> Result<Self, NoReasons> {
        let why = why.into().trim().to_owned();
        if why.is_empty() {
            return Err(NoReasons::Silent);
        }
        if sources.is_empty() {
            return Err(NoReasons::Groundless);
        }
        Ok(Self { why, sources })
    }

    /// What this is for, in the secretary's own words.
    #[must_use]
    pub fn why(&self) -> &str {
        &self.why
    }

    /// What it rests on. Never empty.
    #[must_use]
    pub fn sources(&self) -> &[Reference] {
        &self.sources
    }

    /// These reasons resting on `source` first, and on it once.
    #[must_use]
    pub fn citing(mut self, source: Reference) -> Self {
        self.sources.retain(|held| *held != source);
        self.sources.insert(0, source);
        self
    }
}

impl TryFrom<RawReasons> for Reasons {
    type Error = NoReasons;
    fn try_from(raw: RawReasons) -> Result<Self, NoReasons> {
        Self::new(raw.why, raw.sources)
    }
}

impl From<Reasons> for RawReasons {
    fn from(reasons: Reasons) -> RawReasons {
        RawReasons {
            why: reasons.why,
            sources: reasons.sources,
        }
    }
}
