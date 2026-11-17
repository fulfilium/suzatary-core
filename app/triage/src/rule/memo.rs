//! The firings a process has made: the grow-only set that makes a duplicate notice idempotent.

use std::collections::BTreeSet;

use crate::rule::{Firing, Named, Rule};

/// Every firing made, a grow-only set; a rule is due on a reading at most once.
///
/// It lives in memory and is lost with the process. That is enough because the store is the
/// memo that lasts: a rule's recorded outcome moves its reading to one where P no longer holds.
#[derive(Debug, Clone, Default)]
pub(crate) struct Memo(BTreeSet<Firing>);

impl Memo {
    /// The firing `rule` owes `reading` of `key`, marked as made: `Some` exactly when P holds
    /// and this rule never fired on this reading.
    pub fn due<R: Rule>(&mut self, rule: &R, key: &R::Key, reading: &R::Reading) -> Option<Firing> {
        if !rule.holds(reading) {
            return None;
        }
        let firing = Firing::new(rule.name(), key.to_string(), reading.name());
        self.0.insert(firing.clone()).then_some(firing)
    }
}
