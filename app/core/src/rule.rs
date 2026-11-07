//! The box's work as rules: "when a reading has property P, perform E under a `Decision`, record
//! the outcome" (the Prodrome's `docs/design-register-types.md` §6.0, "the host is the same
//! shape").
//!
//! A [`Rule`] is only P: a predicate on a reading of one key. What a rule performs is its
//! interpreter's, and what it records is an object of the store it writes, so the outcome moves
//! the reading and DISCHARGES the rule (the law each rule's tests state). A firing is memoised by
//! the content name of the reading it fired on ([`ReadingName`]), so a second notice of one
//! reading is the first (`Memo`); the memo that survives a restart is the store itself, since a
//! recorded outcome leaves no reading on which P holds.
//!
//! Two ways a rule runs, the line CALM draws. The box's own rules write monotone records (a
//! transcript's lines, a title, a disposition), so they run with no coordination through an
//! [`Engine`], which holds only rules whose witness is `()`. A rule that performs an outward act
//! is non-monotone ("is it still standing, so I may perform?") and its witness is the owner's
//! `Tap`, which only the gate mints: it runs under its store's `Decision`, never through an
//! engine, so a rule the box fires by itself can only PROPOSE such an act.

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
    /// What the rule is about: a conversation, a proposal, the mail queue.
    type Key: std::fmt::Display;
    /// What it reads of one key, named by its content.
    type Reading: Named;
    /// What must be presented before it performs: `()` for the box's own work.
    type Witness;

    /// Its name in a firing and a log line.
    fn name(&self) -> &'static str;

    /// P: whether this reading calls for the effect, and false once the effect's outcome is
    /// recorded. A function of the reading and of the rule's own fields: a rule an [`Engine`]
    /// memoises reads nothing else, so its firing is a function of the reading's name.
    ///
    /// The tap's rule is the one exception, and it is outside every memo: it reads the instant
    /// it is fired at (an act's expired code stands again), which its reading's name leaves out,
    /// and it runs only under its store's `Decision`, never through an engine. The fence is not
    /// this doc: it is [`Engine`]'s `Witness = ()` bound and its compile-fail doctest.
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

    /// One step: someone writes an odd object, or the engine is told the store may have moved.
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
        /// is noticed: every firing is on a distinct reading, and every noticed reading on which
        /// P held was fired on.
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

        /// A recorded outcome discharges the rule, so a fresh engine over the store (a restart,
        /// with an empty memo) fires nothing on what was already answered.
        #[test]
        fn a_restart_refires_nothing_whose_outcome_is_recorded(script in steps()) {
            let mut script = script;
            script.push(Step::Notice);
            let (store, _, _) = run(&script, true);
            let mut restarted = Engine::new(vec![Owed]);
            proptest::prop_assert!(restarted.due(&"store", &store).is_none());
        }

        /// A reading's name is a function of the set of names: order and repeats do not move it.
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

    /// Two different sets are two names, the empty set included, and the boundary between names
    /// is part of what is hashed.
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
