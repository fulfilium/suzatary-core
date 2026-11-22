//! Which redaction rules a slot of a conversation line was scrubbed under before it was locked.
//!
//! The slot's associated data covers the rules, so a slot cannot claim rules it was not locked
//! under.
//! A slot under older rules is scrubbed again as it opens, until `suzatary secretary
//! migrate-scrub`, the standing step of every change to the rules, locks it again.
//! A slot stored as `Locked(key, box)` reads as unversioned.

use suzatary_base::rule_version::Under;
use suzatary_held::redact::Scrub;

pub type ScrubbedUnder = Under<Scrub>;
