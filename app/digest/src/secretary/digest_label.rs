//! What every node of the digest carries up the tree: whether the lines it spans can hold
//! somebody else's words, and which kinds of line they are.
//!
//! A line's taint is its round's so far, so a model's answer after a mail read is tainted. A
//! node's label is the join of its lines', computed from the lines alone, never told by the
//! model that summarised them.

use prodrome_digest::Measure;

use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::{Entry, Retired};
use suzatary_conversations::secretary::taint::Taint;
use suzatary_conversations::secretary::turn_shape::TurnShape;
use suzatary_inbox::secretary::stamp::Stamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Label {
    pub taint: Taint,
    pub kinds: Kinds,
}

/// A kind of stored line: an [`Entry`]'s constructor, a said line by whose turn it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Opened,
    Titled,
    /// The owner's, or a person's, words.
    Person,
    Model,
    /// A verb's answer.
    Tool,
    Act,
    /// The box's own note, an act line it composed from its own state ([`Entry::noted`]).
    Noted,
    Proposed,
    Reviewed,
    ReviewFailed,
    Stopped,
    Rewound,
    MemorySeen,
    /// A line of the box's bookkeeping: a round's claim, a scope, a reseal, a retired line.
    Session,
    Coding,
    /// An erasure's line, or a line whose words were erased.
    Erased,
}

impl Kind {
    pub const ALL: [Kind; 16] = [
        Kind::Opened,
        Kind::Titled,
        Kind::Person,
        Kind::Model,
        Kind::Tool,
        Kind::Act,
        Kind::Noted,
        Kind::Proposed,
        Kind::Reviewed,
        Kind::ReviewFailed,
        Kind::Stopped,
        Kind::Rewound,
        Kind::MemorySeen,
        Kind::Session,
        Kind::Coding,
        Kind::Erased,
    ];

    #[must_use]
    pub fn of(entry: &Entry) -> Self {
        match entry {
            Entry::Opened { .. } | Entry::SetUp { .. } => Kind::Opened,
            Entry::Titled { .. } => Kind::Titled,
            Entry::Said { turn, by, .. } => match turn {
                Turn::Person { .. } => Kind::Person,
                Turn::Model { .. } => Kind::Model,
                Turn::Tool { .. } => Kind::Tool,
                Turn::Act { .. } => Kind::act(by.as_ref()),
                Turn::Erased { shape, .. } => match shape {
                    TurnShape::Person { .. } => Kind::Person,
                    TurnShape::Model { .. } => Kind::Model,
                    TurnShape::Tool { .. } => Kind::Tool,
                    TurnShape::Act { .. } => Kind::act(by.as_ref()),
                },
            },
            Entry::Proposed { .. } => Kind::Proposed,
            Entry::Reviewed { .. } => Kind::Reviewed,
            Entry::ReviewFailed { .. } => Kind::ReviewFailed,
            Entry::Stopped { .. } => Kind::Stopped,
            Entry::Rewound { .. } | Entry::Again { .. } => Kind::Rewound,
            Entry::Retired {
                retired: Retired::MemorySeen,
                ..
            } => Kind::MemorySeen,
            Entry::Retired {
                retired: Retired::Session | Retired::Unheard { .. },
                ..
            }
            | Entry::Scoped { .. }
            | Entry::Resealed { .. }
            | Entry::Began { .. } => Kind::Session,
            Entry::Coding { .. } => Kind::Coding,
            Entry::Erased { .. } => Kind::Erased,
        }
    }

    pub(crate) fn act(by: Option<&Stamp>) -> Self {
        match by {
            Some(Stamp::Box) => Kind::Noted,
            Some(Stamp::Actor(_) | Stamp::Unconfirmed(_) | Stamp::Asking(_)) | None => Kind::Act,
        }
    }
}

impl suzatary_base::finite::Finite for Kind {
    const ALL: &'static [Self] = &Kind::ALL;
}

pub type Kinds = suzatary_base::finite::Set<Kind>;

impl Label {
    /// The mark of a line whose words were erased, clean and of that kind alone.
    #[must_use]
    pub fn erased() -> Self {
        Self {
            taint: Taint::Clean,
            kinds: Kinds::one(Kind::Erased),
        }
    }

    #[must_use]
    pub fn of(entry: &Entry, taint: Taint) -> Self {
        Self {
            taint,
            kinds: Kinds::one(Kind::of(entry)),
        }
    }
}

/// Which parts a reader is shown in words: each is a down-set of the semilattice, so a summary of
/// lines a reader may read is one it may read.
impl Label {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.taint == Taint::Clean
    }

    /// The owner's own words: clean, and nothing but his turns, acts and erasures, never the
    /// box's notes.
    #[must_use]
    pub fn is_owners(&self) -> bool {
        self.is_clean()
            && self
                .kinds
                .iter()
                .all(|kind| matches!(kind, Kind::Person | Kind::Act | Kind::Erased))
    }
}

impl Measure for Label {
    fn empty() -> Self {
        Self::default()
    }

    fn join(&self, other: &Self) -> Self {
        Self {
            taint: self.taint.join(other.taint),
            kinds: self.kinds.join(&other.kinds),
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn label() -> impl Strategy<Value = Label> {
        (any::<bool>(), any::<u32>()).prop_map(|(tainted, bits)| Label {
            taint: if tainted {
                Taint::Tainted
            } else {
                Taint::Clean
            },
            kinds: Kind::ALL
                .into_iter()
                .enumerate()
                .filter(|(at, _)| bits & (1 << at) != 0)
                .map(|(_, kind)| kind)
                .collect(),
        })
    }

    proptest! {
        /// A semilattice with a unit: associative, commutative, idempotent, `empty` its unit.
        #[test]
        fn labels_form_a_semilattice(a in label(), b in label(), c in label()) {
            prop_assert_eq!(a.join(&b).join(&c), a.join(&b.join(&c)));
            prop_assert_eq!(a.join(&b), b.join(&a));
            prop_assert_eq!(a.join(&a), a);
            prop_assert_eq!(a.join(&Label::empty()), a);
        }
    }

    proptest! {
        /// What a reader may read of a span is what it may read of each of its lines.
        #[test]
        fn a_span_reads_exactly_when_its_parts_do(a in label(), b in label()) {
            prop_assert_eq!(a.join(&b).is_clean(), a.is_clean() && b.is_clean());
            prop_assert_eq!(a.join(&b).is_owners(), a.is_owners() && b.is_owners());
            prop_assert!(Label::empty().is_owners());
        }
    }
}
