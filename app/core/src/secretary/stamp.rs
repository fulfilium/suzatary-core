//! Who stamped a said line: an actor the gate admitted, or the box writing its own note. A line
//! with no stamp relays words the box did not compose. Read once, where a line is parsed, so
//! nothing past the boundary compares a free-text name against a reserved spelling; decided once,
//! where the box writes the line ([`pointing`]), so `taint` is a pure read of stamps.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::chain_set::ChainSet;
use crate::reference::{Reference, TodoRef};
use crate::secretary::own_round::OwnRound;
use crate::signatures::Seat;

/// A said line's stamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Stamp {
    /// An actor the gate admitted: the owner on one of his devices or at the box's terminal.
    Actor(Speaker),
    /// The box, writing a sentence it composed from its own state.
    Box,
    /// An actor the gate admitted, on a line that points at a todo no trusted actor confirmed:
    /// its id, which the model is shown beside the line, may have been chosen by mail the agent
    /// read, so the line is his and is not clean.
    Unconfirmed(Speaker),
    /// The box, asking the question of one of its own rounds. Its words may carry what it did not
    /// compose (a sweep loads mail into its question), so it is not clean, and it is nobody's own.
    Asking(OwnRound),
}

/// An actor the gate admitted, and where the request was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Speaker {
    /// The actor.
    pub actor: String,
    /// Where it asked from.
    pub seat: Seat,
}

impl Speaker {
    /// `actor` on one of the owner's devices.
    #[must_use]
    pub fn device(actor: &str) -> Self {
        Self::at(actor, Seat::Device)
    }

    /// `actor`, asking from `seat`.
    #[must_use]
    pub fn at(actor: &str, seat: Seat) -> Self {
        Self {
            actor: actor.to_owned(),
            seat,
        }
    }
}

/// How [`Stamp::Box`] is printed in a stored line: not an actor's name (an actor matches
/// `^[a-z][a-z0-9_-]*$`), so no gate's stamp reads as the box's.
const BOX: &str = "the box";

/// What follows an actor's name in a stored [`Stamp::Unconfirmed`]: a space, which no actor's name
/// holds, so it never reads as an actor's own stamp.
const UNCONFIRMED: &str = " pointing at an unconfirmed todo";

/// What follows an actor's name in a stored stamp of the operator's, before any
/// [`UNCONFIRMED`]: a space again, so it never reads as an actor's own name.
const OPERATOR: &str = " at the box's terminal";

/// What a stored [`Stamp::Asking`] begins with, before its round's word: spaces and a comma, which
/// no actor's name holds.
const ASKING: &str = "the box, asking the ";

impl From<String> for Stamp {
    fn from(printed: String) -> Self {
        let asking = printed
            .strip_prefix(ASKING)
            .and_then(|word| OwnRound::ALL.into_iter().find(|round| round.word() == word));
        if printed == BOX {
            Stamp::Box
        } else if let Some(round) = asking {
            Stamp::Asking(round)
        } else if let Some(speaker) = printed.strip_suffix(UNCONFIRMED) {
            Stamp::Unconfirmed(Speaker::from(speaker))
        } else {
            Stamp::Actor(Speaker::from(printed.as_str()))
        }
    }
}

impl From<&str> for Speaker {
    fn from(printed: &str) -> Self {
        match printed.strip_suffix(OPERATOR) {
            Some(actor) => Speaker::at(actor, Seat::Operator),
            None => Speaker::device(printed),
        }
    }
}

impl From<&Speaker> for String {
    fn from(speaker: &Speaker) -> Self {
        match speaker.seat {
            Seat::Device => speaker.actor.clone(),
            Seat::Operator => format!("{}{OPERATOR}", speaker.actor),
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
            Stamp::Actor(speaker) => String::from(&speaker),
            Stamp::Box => BOX.to_owned(),
            Stamp::Unconfirmed(speaker) => String::from(&speaker) + UNCONFIRMED,
            Stamp::Asking(round) => format!("{ASKING}{}", round.word()),
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
            Some(Stamp::Box | Stamp::Asking(_)) | None => false,
        }
    }

    /// The actor, when the gate stamped it.
    #[must_use]
    pub fn actor(&self) -> Option<&str> {
        match self {
            Stamp::Actor(speaker) | Stamp::Unconfirmed(speaker) => Some(&speaker.actor),
            Stamp::Box | Stamp::Asking(_) => None,
        }
    }
}

/// The stamp the gate's `speaker` puts on a line pointing at `references`, as `set` reads the chains
/// when the line is written: the actor's own, unless a reference names a todo whose content is
/// still an untrusted writer's ([`crate::view::provisional_content`]), or one the set cannot show
/// to be otherwise, which makes it [`Stamp::Unconfirmed`]. What decides is who wrote the todo's
/// words, its id among them; an untrusted claim about a todo he wrote leaves them his.
#[must_use]
pub fn pointing(speaker: Speaker, references: &[Reference], set: &ChainSet<'_>) -> Stamp {
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
        Stamp::Actor(speaker)
    } else {
        Stamp::Unconfirmed(speaker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stamp prints and reads back as itself, the box's own rounds' included, an actor's name
    /// never reads as the box, and a device's stamp prints as it did before the operator's existed.
    #[test]
    fn a_stamp_reads_back_as_itself() {
        for seat in [Seat::Device, Seat::Operator] {
            for stamp in [
                Stamp::Box,
                Stamp::Actor(Speaker::at("ada", seat)),
                Stamp::Unconfirmed(Speaker::at("ada", seat)),
            ] {
                assert_eq!(Stamp::from(String::from(stamp.clone())), stamp);
            }
        }
        for round in OwnRound::ALL {
            let stamp = Stamp::Asking(round);
            assert_eq!(Stamp::from(String::from(stamp.clone())), stamp);
        }
        assert_eq!(Stamp::from("box"), Stamp::Actor(Speaker::device("box")));
        assert_eq!(String::from(Stamp::Actor(Speaker::device("ada"))), "ada");
        assert_eq!(
            Stamp::from("ada at the box's terminal pointing at an unconfirmed todo"),
            Stamp::Unconfirmed(Speaker::at("ada", Seat::Operator))
        );
    }
}
