//! The engine: the box's own rules over one key space, each fired once per reading.

use crate::rule::{Firing, Memo, Rule};

/// Rules over one kind of key and reading, tried in the order declared, sharing one `Memo`.
///
/// It holds only rules whose witness is `()`: an outward act needs the owner's `Tap`, which the
/// box cannot mint, so no rule it fires by itself can perform one.
///
/// ```
/// use suzatary_core::rule::{Engine, Named, ReadingName, Rule};
/// struct Reading;
/// impl Named for Reading {
///     fn name(&self) -> ReadingName { ReadingName::of(["a"]) }
/// }
/// #[derive(Clone)]
/// struct Inward;
/// impl Rule for Inward {
///     type Key = String;
///     type Reading = Reading;
///     type Witness = ();
///     fn name(&self) -> &'static str { "inward" }
///     fn holds(&self, _: &Reading) -> bool { true }
/// }
/// let mut engine = Engine::new(vec![Inward]);
/// assert!(engine.due(&"k".to_owned(), &Reading).is_some());
/// assert!(engine.due(&"k".to_owned(), &Reading).is_none(), "once per reading");
/// ```
///
/// The same rule with a witness the box cannot make does not compile into one:
///
/// ```compile_fail
/// use suzatary_core::rule::{Engine, Named, ReadingName, Rule};
/// struct Reading;
/// impl Named for Reading {
///     fn name(&self) -> ReadingName { ReadingName::of(["a"]) }
/// }
/// /// What only a gate could mint.
/// struct Witness;
/// #[derive(Clone)]
/// struct Outward;
/// impl Rule for Outward {
///     type Key = String;
///     type Reading = Reading;
///     type Witness = Witness;
///     fn name(&self) -> &'static str { "outward" }
///     fn holds(&self, _: &Reading) -> bool { true }
/// }
/// let _ = Engine::new(vec![Outward]);
/// ```
#[derive(Debug, Clone)]
pub struct Engine<R: Rule<Witness = ()>> {
    rules: Vec<R>,
    memo: Memo,
}

impl<R: Rule<Witness = ()> + Clone> Engine<R> {
    /// These rules, with nothing fired yet.
    #[must_use]
    pub fn new(rules: Vec<R>) -> Self {
        Self {
            rules,
            memo: Memo::default(),
        }
    }

    /// The first rule due on `reading` of `key`, and its firing, marked as made.
    pub fn due(&mut self, key: &R::Key, reading: &R::Reading) -> Option<(R, Firing)> {
        let Self { rules, memo } = self;
        rules.iter().find_map(|rule| {
            memo.due(rule, key, reading)
                .map(|firing| (rule.clone(), firing))
        })
    }
}
