//! A store's own rule for what a tab posts, beyond the shared rule (posted as its poster, signed
//! by its writer's key, resting on what is held or posted with it).
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
    /// Held back, with everything resting on it.
    Final,
    /// Sealed under rules older than the keeper's, so a newer writer seals it again.
    Outdated,
    /// Posted again later, as it is, and taken then if the rule takes it.
    Later,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inadmissible {
    pub grounds: Ruling,
    pub reason: String,
}

impl Inadmissible {
    #[must_use]
    pub fn finally(reason: impl Into<String>) -> Self {
        Self {
            grounds: Ruling::Final,
            reason: reason.into(),
        }
    }

    #[must_use]
    pub fn later(reason: impl Into<String>) -> Self {
        Self {
            grounds: Ruling::Later,
            reason: reason.into(),
        }
    }
}

/// What a post carries beside its objects, parsed once at the door: by object name, its strings
/// in order.
pub trait Beside: Sized {
    /// # Errors
    /// The sentence a refusal of the whole post says.
    fn parse(posted: &BTreeMap<String, Vec<String>>) -> Result<Self, String>;
}

impl Beside for () {
    fn parse(posted: &BTreeMap<String, Vec<String>>) -> Result<Self, String> {
        if posted.is_empty() {
            Ok(())
        } else {
            Err("this store takes nothing beside its objects".to_owned())
        }
    }
}

/// What a rule reads of the post a change came in; a post is judged one change at a time.
pub struct Post<'a, 'e, A: Admission + ?Sized> {
    pub name: &'a Hash,
    pub beside: &'a A::Beside,
    /// The store as read once for the post, with each earlier change of the post taken in.
    pub held: &'a A::Held<'e>,
    pub world: &'a A::World,
    /// The keys the post's signatures of this change name; the shared rule proves them.
    pub signers: &'a [prodrome_core::sign::PublicKey],
}

/// A store's own rule for what a tab posts to it.
pub trait Admission {
    type Schema: Schema;

    type Beside: Beside;

    type Held<'e>
    where
        Self::Schema: 'e;

    type World: ?Sized;

    /// Made once per post under the store's lock.
    fn held(history: &Dag<Self::Schema>) -> Self::Held<'_>;

    /// Why the box refuses `change` beyond the shared rule; `None` to take it.
    fn refuses(change: &Change<Self::Schema>, post: &Post<'_, '_, Self>) -> Option<Inadmissible>;

    /// `held` with `change` taken in, for judging the post's next change.
    fn take(held: &mut Self::Held<'_>, name: &Hash, change: &Change<Self::Schema>);
}

/// The shared rule alone.
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
