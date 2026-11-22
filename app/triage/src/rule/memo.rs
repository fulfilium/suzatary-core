use std::collections::BTreeSet;

use crate::rule::{Firing, Named, Rule};

/// Every firing made, in memory only: enough, because the store is the memo that lasts, a
/// recorded outcome moving its reading to one where P no longer holds.
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
