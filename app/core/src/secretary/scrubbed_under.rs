//! Which redaction rules a slot of a conversation line was scrubbed under before it was locked.
//!
//! A line records it beside each slot's words, and the slot's associated data covers it, so a
//! slot cannot claim rules it was not locked under. A slot under the current rules is read as it
//! was locked; any other is scrubbed again as it opens, until `suzatary secretary migrate-scrub`,
//! the standing step of every change to the rules, locks it again under the current ones.

use crate::redact::Rules;

/// The rules a slot's words passed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScrubbedUnder {
    /// The rules [`Rules`] names.
    Rules(Rules),
    /// Rules from before a line recorded them: the slot is stored in the form `Locked(key, box)`.
    Unversioned,
}

impl ScrubbedUnder {
    /// Whether these are the rules this build scrubs under.
    #[must_use]
    pub fn is_current(&self) -> bool {
        match self {
            ScrubbedUnder::Rules(rules) => rules == Rules::current(),
            ScrubbedUnder::Unversioned => false,
        }
    }
}
