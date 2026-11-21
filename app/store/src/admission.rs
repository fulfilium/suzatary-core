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
use prodrome_core::dag::Dag;
use prodrome_core::event::Hash;
use prodrome_core::schema::Schema;

/// On which grounds a store's own rule refuses a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ruling {
    /// Never taken as it is: held back, with everything resting on it.
    Final,
    /// Sealed under rules older than the keeper's, so a newer writer seals it again.
    Outdated,
    /// Not yet: posted again later, as it is, and taken then if the rule takes it.
    Later,
}

/// A change a store's own rule refuses: why it is held back, in the rule's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inadmissible {
    /// On which grounds.
    pub grounds: Ruling,
    /// The rule's sentence.
    pub reason: String,
}

impl Inadmissible {
    /// Refused for good, in `reason`.
    #[must_use]
    pub fn finally(reason: impl Into<String>) -> Self {
        Self {
            grounds: Ruling::Final,
            reason: reason.into(),
        }
    }

    /// Refused for now, in `reason`: the writer posts it again later.
    #[must_use]
    pub fn later(reason: impl Into<String>) -> Self {
        Self {
            grounds: Ruling::Later,
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

/// What a rule reads of the post a change came in: its name, what the post carries beside it, and
/// what the rule read of the store it rules, with every change of the post taken before this one,
/// and of the world beyond it. Never the rest of the post: a post is judged one change at a time.
pub struct Post<'a, 'e, A: Admission + ?Sized> {
    /// The change's name.
    pub name: &'a Hash,
    /// What the post carries beside its objects.
    pub beside: &'a A::Beside,
    /// What the rule read of the store, once for the post ([`Admission::held`]), and each change
    /// of the post taken before this one ([`Admission::take`]).
    pub held: &'a A::Held<'e>,
    /// What the rule reads beyond its own store.
    pub world: &'a A::World,
    /// The keys the post's signatures of this change name; the shared rule proves them.
    pub signers: &'a [prodrome_core::sign::PublicKey],
}

/// A store's own rule for what a tab posts to it.
pub trait Admission {
    /// The schema of the store it rules.
    type Schema: Schema;

    /// What a post carries beside its objects for this store.
    type Beside: Beside;

    /// What the rule reads of the store it rules, borrowing the objects read.
    type Held<'e>
    where
        Self::Schema: 'e;

    /// What the rule reads beyond its own store, as the box holds it now.
    type World: ?Sized;

    /// That reading, of every object the store holds by name, made once per post under the
    /// store's lock.
    fn held(history: &Dag<Self::Schema>) -> Self::Held<'_>;

    /// Why the box refuses `change` beyond the shared rule, in its own words; none to take it.
    fn refuses(change: &Change<Self::Schema>, post: &Post<'_, '_, Self>) -> Option<Inadmissible>;

    /// `held` with `change`, named `name`, taken in: what the next change of the post is judged
    /// against.
    fn take(held: &mut Self::Held<'_>, name: &Hash, change: &Change<Self::Schema>);
}

/// The shared rule alone, for a store whose writes need nothing more.
#[derive(Debug, Clone, Copy, Default)]
pub struct Shared<E>(PhantomData<E>);

impl<E: Schema> Admission for Shared<E> {
    type Schema = E;
    type Beside = ();
    type Held<'e>
        = ()
    where
        E: 'e;
    type World = ();

    fn held(_: &Dag<E>) {}

    fn refuses(_: &Change<E>, _: &Post<'_, '_, Self>) -> Option<Inadmissible> {
        None
    }

    fn take((): &mut (), _: &Hash, _: &Change<E>) {}
}
