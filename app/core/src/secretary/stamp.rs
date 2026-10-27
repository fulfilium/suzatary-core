//! Who stamped a said line: an actor the gate admitted, or the box writing its own note. A line
//! with no stamp relays words the box did not compose. Read once, where a line is parsed, so
//! nothing past the boundary compares a free-text name against a reserved spelling; decided once,
//! where the box writes the line ([`pointing`]), so `taint` is a pure read of stamps.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::chain_set::ChainSet;
use crate::reference::{Reference, TodoRef};

/// A said line's stamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Stamp {
    /// An actor the gate admitted: the owner on one of his devices.
    Actor(String),
    /// The box, writing a sentence it composed from its own state.
    Box,
    /// An actor the gate admitted, on a line that points at a todo no trusted actor confirmed:
    /// its id, which the model is shown beside the line, may have been chosen by mail the agent
    /// read, so the line is his and is not clean.
    Unconfirmed(String),
}

/// How [`Stamp::Box`] is printed in a stored line: not an actor's name (an actor matches
/// `^[a-z][a-z0-9_-]*$`), so no gate's stamp reads as the box's.
const BOX: &str = "the box";

/// What follows an actor's name in a stored [`Stamp::Unconfirmed`]: a space, which no actor's name
/// holds, so it never reads as an actor's own stamp.
const UNCONFIRMED: &str = " pointing at an unconfirmed todo";

impl From<String> for Stamp {
    fn from(printed: String) -> Self {
        if printed == BOX {
            Stamp::Box
        } else if let Some(actor) = printed.strip_suffix(UNCONFIRMED) {
            Stamp::Unconfirmed(actor.to_owned())
        } else {
            Stamp::Actor(printed)
        }
    }
}

prodrome_core::text_literal!(
    Stamp => |stamp| String::from(stamp.clone()),
);

impl From<&str> for Stamp {
    fn from(printed: &str) -> Self {
        Stamp::from(printed.to_owned())
    }
}

impl From<Stamp> for String {
    fn from(stamp: Stamp) -> Self {
        match stamp {
            Stamp::Actor(actor) => actor,
            Stamp::Box => BOX.to_owned(),
            Stamp::Unconfirmed(actor) => actor + UNCONFIRMED,
        }
    }
}

impl Stamp {
    /// Whether a line stamped `by` is an actor's, as the gate stamped it: the owner's. A line the
    /// box wrote, stamped or relaying another's words unstamped, is not.
    #[must_use]
    pub fn owners(by: Option<&Self>) -> bool {
        match by {
            Some(Stamp::Actor(_) | Stamp::Unconfirmed(_)) => true,
            Some(Stamp::Box) | None => false,
        }
    }

    /// The actor, when the gate stamped it.
    #[must_use]
    pub fn actor(&self) -> Option<&str> {
        match self {
            Stamp::Actor(actor) | Stamp::Unconfirmed(actor) => Some(actor),
            Stamp::Box => None,
        }
    }
}

/// The stamp the gate's `actor` puts on a line pointing at `references`, as `set` reads the chains
/// when the line is written: the actor's own, unless a reference names a todo whose content is
/// still an untrusted writer's ([`crate::view::provisional_content`]), or one the set cannot show
/// to be otherwise, which makes it [`Stamp::Unconfirmed`]. What decides is who wrote the todo's
/// words, its id among them; an untrusted claim about a todo he wrote leaves them his.
#[must_use]
pub fn pointing(actor: &str, references: &[Reference], set: &ChainSet<'_>) -> Stamp {
    let mut todos: BTreeMap<_, BTreeSet<&str>> = BTreeMap::new();
    for reference in references {
        if let Reference::Todo(TodoRef { chain, id }) = reference {
            todos.entry(chain).or_default().insert(id.as_str());
        }
    }
    let confirmed = todos.into_iter().all(|(chain, ids)| {
        set.entries(chain).is_ok_and(|rows| {
            ids.iter().all(|id| {
                rows.iter()
                    .any(|row| row.key.as_str() == *id && !crate::view::provisional_content(row))
            })
        })
    });
    if confirmed {
        Stamp::Actor(actor.to_owned())
    } else {
        Stamp::Unconfirmed(actor.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stamp prints and reads back as itself, and an actor's name never reads as the box.
    #[test]
    fn a_stamp_reads_back_as_itself() {
        for stamp in [
            Stamp::Box,
            Stamp::Actor("ada".to_owned()),
            Stamp::Unconfirmed("ada".to_owned()),
        ] {
            assert_eq!(Stamp::from(String::from(stamp.clone())), stamp);
        }
        assert_eq!(Stamp::from("box"), Stamp::Actor("box".to_owned()));
    }
}
