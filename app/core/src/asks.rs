//! The asks store's schema: the box's refusal of an object a tab posted, named by the object's
//! name and the store it was posted to, which every device of the owner pulls. A refused object
//! leaves each reading of that store with everything resting on it.

use crate::receipt::{RefusalEvent, RefusalVocabulary};
use crate::stores::Kind;

prodrome_core::word_literal!(Kind: Kind::ALL.iter().copied() => |kind| kind.name());

/// The actor the box writes every refusal as.
#[must_use]
pub fn keeper() -> prodrome_core::event::Actor {
    prodrome_core::event::Actor::new("box").expect("a literal actor name")
}

/// One event of the asks store.
pub type AskEvent = RefusalEvent<Kind>;

/// The asks store's whitelist.
pub type AskVocabulary = RefusalVocabulary<Kind>;
