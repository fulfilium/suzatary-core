//! The box's work as rules: a [`Rule`] is a predicate P on a reading of one key, and the outcome
//! its interpreter records moves the reading and DISCHARGES the rule (the law each rule's tests
//! state).
//!
//! A firing is memoised by the reading's content name ([`ReadingName`]); the memo that survives a
//! restart is the store itself, since a recorded outcome leaves no reading on which P holds.
//!
//! A rule that performs an outward act is not monotone, so its witness is the owner's `Tap` and it
//! runs under its store's `Decision`: a rule the box fires by itself can only PROPOSE it.

mod engine;
mod firing;
mod memo;
mod reading_name;

pub use engine::Engine;
pub use firing::Firing;
pub(crate) use memo::Memo;
pub use reading_name::{Named, ReadingName};

/// P over a reading of one key: whether the box owes that reading its effect.
pub trait Rule {
    type Key: std::fmt::Display;
    type Reading: Named;
    /// What must be presented before it performs: `()` for the box's own work.
    type Witness;

    fn name(&self) -> &'static str;

    /// P, false once the effect's outcome is recorded; a rule an [`Engine`] memoises reads only
    /// the reading and its own fields, so its firing is a function of the reading's name.
    ///
    /// The tap's rule reads the instant it is fired at, so it runs only under its store's
    /// `Decision`, never through an engine; the fence is [`Engine`]'s `Witness = ()` bound.
    fn holds(&self, reading: &Self::Reading) -> bool;
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// A store of numbered objects; an odd greatest object is owed an answer, which is the next
    /// even one.
    #[derive(Debug, Clone, Default)]
    struct Objects(BTreeSet<u32>);

    impl Named for Objects {
        fn name(&self) -> ReadingName {
            ReadingName::of(self.0.iter().map(u32::to_string))
        }
    }

    impl Objects {
        fn answer(&mut self) {
            let last = self.0.iter().next_back().copied().unwrap_or(0);
            self.0.insert(last + 1);
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct Owed;

    impl Rule for Owed {
        type Key = &'static str;
        type Reading = Objects;
        type Witness = ();

        fn name(&self) -> &'static str {
            "owed"
        }

        fn holds(&self, reading: &Objects) -> bool {
            reading
                .0
                .iter()
                .next_back()
                .is_some_and(|last| last % 2 == 1)
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum Step {
        Write(u32),
        Notice,
    }

    fn steps() -> impl proptest::strategy::Strategy<Value = Vec<Step>> {
        use proptest::prelude::*;
        proptest::collection::vec(
            prop_oneof![
                (0u32..1000).prop_map(|n| Step::Write(2 * n + 1)),
                Just(Step::Notice),
            ],
            0..40,
        )
    }

    /// Runs a script; `records` says whether a firing's effect writes its outcome.
    fn run(script: &[Step], records: bool) -> (Objects, Vec<Firing>, Vec<ReadingName>) {
        let mut store = Objects::default();
        let mut engine = Engine::new(vec![Owed]);
        let mut fired = Vec::new();
        let mut owed_seen = Vec::new();
        for step in script {
            match step {
                Step::Write(n) => {
                    store.0.insert(*n);
                }
                Step::Notice => {
                    if Owed.holds(&store) {
                        owed_seen.push(store.name());
                    }
                    if let Some((_, firing)) = engine.due(&"store", &store) {
                        fired.push(firing);
                        if records {
                            store.answer();
                        }
                    }
                }
            }
        }
        (store, fired, owed_seen)
    }

    proptest::proptest! {
        /// A rule fires exactly once per reading that satisfies it, however often that reading
        /// is noticed.
        #[test]
        fn a_rule_fires_exactly_once_per_reading_that_satisfies_it(script in steps()) {
            for records in [true, false] {
                let (_, fired, owed_seen) = run(&script, records);
                let on: Vec<&ReadingName> = fired.iter().map(Firing::on).collect();
                let distinct: BTreeSet<&ReadingName> = on.iter().copied().collect();
                proptest::prop_assert_eq!(on.len(), distinct.len(), "a reading fired twice");
                let seen: BTreeSet<&ReadingName> = owed_seen.iter().collect();
                proptest::prop_assert_eq!(distinct, seen);
            }
        }

        /// A recorded outcome discharges the rule, so a restart fires nothing already answered.
        #[test]
        fn a_restart_refires_nothing_whose_outcome_is_recorded(script in steps()) {
            let mut script = script;
            script.push(Step::Notice);
            let (store, _, _) = run(&script, true);
            let mut restarted = Engine::new(vec![Owed]);
            proptest::prop_assert!(restarted.due(&"store", &store).is_none());
        }

        #[test]
        fn a_readings_name_is_a_function_of_the_set(
            names in proptest::collection::vec("[a-f0-9]{1,8}", 0..12),
            seed in proptest::prelude::any::<u64>(),
        ) {
            let mut shuffled = names.clone();
            shuffled.extend(names.iter().take(3).cloned());
            let k = usize::try_from(seed).unwrap_or(0) % (shuffled.len().max(1));
            shuffled.rotate_left(k);
            shuffled.reverse();
            proptest::prop_assert_eq!(ReadingName::of(names), ReadingName::of(shuffled));
        }
    }

    /// The boundary between names is part of what is hashed.
    #[test]
    fn different_sets_are_different_names() {
        let empty = ReadingName::of(Vec::<String>::new());
        assert_ne!(empty, ReadingName::of(["a"]));
        assert_ne!(ReadingName::of(["ab", "c"]), ReadingName::of(["a", "bc"]));
    }

    /// The name's bytes are fixed (sha256 over each name's 8-byte big-endian length and bytes,
    /// sorted), so the box and the tab's wasm name one reading alike.
    #[test]
    fn a_readings_name_is_this_vector() {
        assert_eq!(
            ReadingName::of(["bc", "a", "bc"]).as_str(),
            "3fafa1cf2f19a7c1129beb20cf0983f73a489a221fc0dd2f16d1be292d089205"
        );
    }
}
