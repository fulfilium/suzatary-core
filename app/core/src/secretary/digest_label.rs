//! What every node of the digest carries up the tree: whether the lines it spans can hold
//! somebody else's words, and which kinds of line they are.
//!
//! A line's taint is its round's so far (`taint::rounds`), so a model's answer after a mail read
//! is tainted. A [`Label`] is a join-semilattice with a unit, so the Prodrome's [`Measure`]: a node's label
//! is the join of its lines', whatever the tree's shape, and it is computed by the reading from
//! the lines alone, never told by the model that summarised them.

use prodrome_core::digest::Measure;

use crate::capability::Turn;
use crate::secretary::store::Entry;
use crate::secretary::taint::Taint;
use crate::secretary::turn_shape::TurnShape;

/// A span of lines' taint and kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Label {
    /// The join of its lines' taints: tainted when any one is.
    pub taint: Taint,
    /// Every kind of line it spans.
    pub kinds: Kinds,
}

/// A kind of stored line: an [`Entry`]'s constructor, a said line by whose turn it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// A conversation opened.
    Opened,
    /// A conversation named.
    Titled,
    /// The owner's, or a person's, words.
    Person,
    /// The model's words.
    Model,
    /// A verb's answer.
    Tool,
    /// An act's line.
    Act,
    /// A proposal made.
    Proposed,
    /// A batch settled.
    Reviewed,
    /// A review that could not run.
    ReviewFailed,
    /// A round stopped.
    Stopped,
    /// Turns set aside.
    Rewound,
    /// The memory's version told.
    MemorySeen,
    /// A line about the model session: opened, kept, ended by a turn it did not hear, or the
    /// scope its asks are sent in.
    Session,
    /// A coding session's line.
    Coding,
    /// An erasure's line, or a line whose words were erased.
    Erased,
}

impl Kind {
    /// Every kind, in order.
    pub const ALL: [Kind; 15] = [
        Kind::Opened,
        Kind::Titled,
        Kind::Person,
        Kind::Model,
        Kind::Tool,
        Kind::Act,
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

    /// The kind of `entry`.
    #[must_use]
    pub fn of(entry: &Entry) -> Self {
        match entry {
            Entry::Opened { .. } => Kind::Opened,
            Entry::Titled { .. } => Kind::Titled,
            Entry::Said { turn, .. } => match turn {
                Turn::Person { .. } => Kind::Person,
                Turn::Model { .. } => Kind::Model,
                Turn::Tool { .. } => Kind::Tool,
                Turn::Act { .. } => Kind::Act,
                Turn::Erased { shape, .. } => match shape {
                    TurnShape::Person { .. } => Kind::Person,
                    TurnShape::Model { .. } => Kind::Model,
                    TurnShape::Tool { .. } => Kind::Tool,
                    TurnShape::Act { .. } => Kind::Act,
                },
            },
            Entry::Proposed { .. } => Kind::Proposed,
            Entry::Reviewed { .. } => Kind::Reviewed,
            Entry::ReviewFailed { .. } => Kind::ReviewFailed,
            Entry::Stopped { .. } => Kind::Stopped,
            Entry::Rewound { .. } => Kind::Rewound,
            Entry::MemorySeen { .. } => Kind::MemorySeen,
            Entry::Session { .. }
            | Entry::Kept { .. }
            | Entry::Scoped { .. }
            | Entry::Unheard { .. }
            | Entry::Resealed { .. } => Kind::Session,
            Entry::Coding { .. } => Kind::Coding,
            Entry::Erased { .. } => Kind::Erased,
        }
    }

    fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// A set of [`Kind`]s, joined by union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Kinds(u16);

impl Kinds {
    /// The set holding `kind` alone.
    #[must_use]
    pub fn one(kind: Kind) -> Self {
        Self(kind.bit())
    }

    /// Whether it holds `kind`.
    #[must_use]
    pub fn contains(self, kind: Kind) -> bool {
        self.0 & kind.bit() != 0
    }

    /// Its kinds, in order.
    pub fn iter(self) -> impl Iterator<Item = Kind> {
        Kind::ALL
            .into_iter()
            .filter(move |kind| self.contains(*kind))
    }

    /// The union.
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl Label {
    /// The mark of a line whose words were erased, clean and of that kind alone.
    #[must_use]
    pub fn erased() -> Self {
        Self {
            taint: Taint::Clean,
            kinds: Kinds::one(Kind::Erased),
        }
    }

    /// One line's: `taint`, its taint in its round ([`crate::secretary::taint::rounds`]), and its kind.
    #[must_use]
    pub fn of(entry: &Entry, taint: Taint) -> Self {
        Self {
            taint,
            kinds: Kinds::one(Kind::of(entry)),
        }
    }
}

/// Which parts a reader is shown in words. Each is a down-set of the semilattice, a homomorphism
/// to `bool` under `and` (a span reads exactly when each of its lines does), so a summary of lines
/// a reader may read is one it may read.
impl Label {
    /// Clean: nothing in it can be somebody else's words. What the compactor reads.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.taint == Taint::Clean
    }

    /// The owner's own words: clean, and nothing but his turns and acts (and erasures, which hold
    /// no words). What every session is
    /// told; the secretary's answers, verbs' answers and the box's bookkeeping are opened by `zoom`.
    #[must_use]
    pub fn is_owners(&self) -> bool {
        self.is_clean()
            && self
                .kinds
                .iter()
                .all(|kind| matches!(kind, Kind::Person | Kind::Act | Kind::Erased))
    }
}

/// Clean and no kind is the unit; the join is componentwise, so commutative and idempotent too.
impl Measure for Label {
    fn empty() -> Self {
        Self::default()
    }

    fn join(&self, other: &Self) -> Self {
        Self {
            taint: self.taint.join(other.taint),
            kinds: self.kinds.join(other.kinds),
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn label() -> impl Strategy<Value = Label> {
        (any::<bool>(), any::<u16>()).prop_map(|(tainted, kinds)| Label {
            taint: if tainted {
                Taint::Tainted
            } else {
                Taint::Clean
            },
            kinds: Kinds(kinds & ((1 << Kind::ALL.len()) - 1)),
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
