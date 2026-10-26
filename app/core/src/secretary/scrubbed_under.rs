//! Which redaction rules a slot of a conversation line was scrubbed under before it was locked.
//!
//! A line records it beside each slot's words, and the slot's associated data covers it, so a
//! slot cannot claim rules it was not locked under. A slot under the current rules is read as it
//! was locked; any other is scrubbed again as it opens, until `suzatary secretary migrate-scrub`,
//! the standing step of every change to the rules, locks it again under the current ones. A slot
//! from before lines recorded their rules is stored in the form `Locked(key, box)`, and reads as
//! unversioned.

use crate::redact::Scrub;
use crate::rule_version::Under;

/// The rules a slot's words passed.
pub type ScrubbedUnder = Under<Scrub>;
