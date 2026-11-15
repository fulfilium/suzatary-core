//! What the box takes of a store a tab writes, beyond the rule every such store shares (a change
//! posted as its poster, taken only beside a signature its writer's key makes, resting on what is
//! held or posted with it): a store's own rule, over its schema. A new kind of write costs its
//! vocabulary and this rule alone.
//!
//! A key travels beside the objects, never inside one: an object is kept forever and a key must be
//! destroyable.

use std::marker::PhantomData;

use prodrome_core::change::Change;
use prodrome_core::schema::Schema;

/// A store's own rule for what a tab posts to it.
pub trait Admission {
    /// The schema of the store it rules.
    type Schema: Schema;

    /// Why the box refuses `change` beyond the shared rule, in its own words; none to take it.
    fn refuses(change: &Change<Self::Schema>) -> Option<String>;
}

/// The shared rule alone, for a store whose writes need nothing more.
#[derive(Debug, Clone, Copy, Default)]
pub struct Shared<E>(PhantomData<E>);

impl<E: Schema> Admission for Shared<E> {
    type Schema = E;

    fn refuses(_: &Change<E>) -> Option<String> {
        None
    }
}
