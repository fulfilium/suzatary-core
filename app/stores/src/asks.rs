//! The asks store's schema: the box's refusal of an object a tab posted, by the object's name and
//! store, which every device of the owner pulls; a refused object leaves each reading of that
//! store with everything resting on it.

use crate::stores::Kind;
use prodrome_data::event::Actor;
use suzatary_store::receipt::{RefusalEvent, RefusalVocabulary};

prodrome_data::word_literal!(Kind: Kind::ALL.iter().copied() => |kind| kind.name());

/// The actor the box writes every refusal as.
#[must_use]
pub fn keeper() -> Actor {
    Actor::new("box").expect("a literal actor name")
}

pub type AskEvent = RefusalEvent<Kind>;

/// The asks store's whitelist.
pub type AskVocabulary = RefusalVocabulary<Kind>;
