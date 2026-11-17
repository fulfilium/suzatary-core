//! What the box takes of a store a tab writes, beyond the rule every such store shares (a change
//! posted as its poster, taken only beside a signature its writer's key makes, resting on what is
//! held or posted with it): a store's own rule, over its schema, and what a post carries beside
//! its objects for it. A new kind of write costs its vocabulary and this rule alone.
//!
//! A key travels beside the objects, never inside one: an object is kept forever and a key must be
//! destroyable.

use std::collections::BTreeMap;
use std::marker::PhantomData;

use prodrome_core::change::Change;
use prodrome_core::event::Hash;
use prodrome_core::schema::Schema;

/// A change a store's own rule refuses: why it is held back, in the rule's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inadmissible {
    /// On which grounds.
    pub grounds: crate::receipt::Grounds,
    /// The rule's sentence.
    pub reason: String,
}

impl Inadmissible {
    /// Refused for good, in `reason`.
    #[must_use]
    pub fn finally(reason: impl Into<String>) -> Self {
        Self {
            grounds: crate::receipt::Grounds::Final,
            reason: reason.into(),
        }
    }
}

/// What a post carries beside its objects, parsed once at the door from the wire's strings: by
/// object name, each string of it in order.
pub trait Beside: Sized {
    /// `posted` read, or why it is no such thing.
    ///
    /// # Errors
    /// The sentence a refusal of the whole post says.
    fn parse(posted: &BTreeMap<String, Vec<String>>) -> Result<Self, String>;
}

/// Nothing beside the objects.
impl Beside for () {
    fn parse(posted: &BTreeMap<String, Vec<String>>) -> Result<Self, String> {
        if posted.is_empty() {
            Ok(())
        } else {
            Err("this store takes nothing beside its objects".to_owned())
        }
    }
}

/// What a rule reads of the post a change came in: its name, what the post carries beside it,
/// what the rule read of the store it rules, and every change the post holds, this one included.
pub struct Post<'a, A: Admission + ?Sized> {
    /// The change's name.
    pub name: &'a Hash,
    /// What the post carries beside its objects.
    pub beside: &'a A::Beside,
    /// What the rule read of the store, once for the post ([`Admission::held`]).
    pub held: &'a A::Held,
    /// Every change of the post.
    pub changes: &'a [&'a Change<A::Schema>],
}

/// A store's own rule for what a tab posts to it.
pub trait Admission {
    /// The schema of the store it rules.
    type Schema: Schema;

    /// What a post carries beside its objects for this store.
    type Beside: Beside;

    /// What the rule reads of the store it rules.
    type Held;

    /// That reading, of every event the store holds, made once per post under the store's lock.
    fn held<'e>(events: impl Iterator<Item = &'e Self::Schema>) -> Self::Held
    where
        Self::Schema: 'e;

    /// Why the box refuses `change` beyond the shared rule, in its own words; none to take it.
    fn refuses(change: &Change<Self::Schema>, post: &Post<'_, Self>) -> Option<Inadmissible>;
}

/// The shared rule alone, for a store whose writes need nothing more.
#[derive(Debug, Clone, Copy, Default)]
pub struct Shared<E>(PhantomData<E>);

impl<E: Schema> Admission for Shared<E> {
    type Schema = E;
    type Beside = ();
    type Held = ();

    fn held<'e>(_: impl Iterator<Item = &'e E>) -> Self::Held
    where
        E: 'e,
    {
    }

    fn refuses(_: &Change<E>, _: &Post<'_, Self>) -> Option<Inadmissible> {
        None
    }
}
