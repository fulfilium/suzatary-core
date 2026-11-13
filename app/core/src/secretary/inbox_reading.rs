//! Where one proposal's machine stands: its state register, read. Standing while nothing answered
//! it, the one response that stands, or the responses that stand at once, a conflict nothing acts
//! on. A check is read as the send that may have gone it checks ([`Check`]); a reading holding a
//! check of nothing is unreadable, and says why, so the card is drawn with its reason and nothing
//! acts on it.

use prodrome_core::event::Hash;
use prodrome_core::literal::Datetime;

use crate::secretary::inbox::{Check, Unchecked};
use crate::secretary::memory::Fact;
use crate::secretary::phase::Phase;
use crate::secretary::responded::Responded;
use crate::secretary::stamp::stamp;

/// One proposal's state register, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    /// Nothing answered it.
    Standing,
    /// One response stands.
    One(Answer),
    /// Responses that neither descends from.
    Conflict(Conflict),
    /// Its responses read as no state, and why: shown, never acted on.
    Unreadable(Unchecked),
}

/// Two or more responses that stand at once, none descending from another, least named first:
/// a conflict of fewer than two is no conflict, so it cannot be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The least named, which a view that shows one record shows.
    pub first: Answer,
    /// The next.
    pub second: Answer,
    /// Every other, in order.
    pub rest: Vec<Answer>,
}

impl Conflict {
    /// Every response, least named first.
    pub fn iter(&self) -> impl Iterator<Item = &Answer> {
        [&self.first, &self.second].into_iter().chain(&self.rest)
    }

    /// How many responses stand: two or more.
    #[must_use]
    pub fn sides(&self) -> usize {
        2 + self.rest.len()
    }
}

/// A response the reading holds: the write's name and instant, and what it said, a check holding
/// the send it checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub name: Hash,
    pub at: Datetime,
    pub response: Responded<Check>,
}

impl Reading {
    /// The answer a view that shows one record shows: the one, or of a conflict the least named.
    #[must_use]
    pub fn shown(&self) -> Option<&Answer> {
        match self {
            Reading::Standing | Reading::Unreadable(_) => None,
            Reading::One(answer) => Some(answer),
            Reading::Conflict(conflict) => Some(&conflict.first),
        }
    }

    /// The responses that stand, least named first: none while standing, two or more in
    /// conflict.
    #[must_use]
    pub fn answers(&self) -> Vec<&Answer> {
        match self {
            Reading::Standing | Reading::Unreadable(_) => Vec::new(),
            Reading::One(answer) => vec![answer],
            Reading::Conflict(conflict) => conflict.iter().collect(),
        }
    }

    /// Whether a tap performs it and the owner may say no: while nothing settled it. A conflict
    /// performs nothing until one response settles it.
    #[must_use]
    pub fn performs(&self) -> bool {
        match self {
            Reading::Standing => true,
            Reading::One(answer) => answer.response.performs(),
            Reading::Conflict(_) | Reading::Unreadable(_) => false,
        }
    }

    /// Whether it is a flow that runs on and ends by itself, as it is shown.
    #[must_use]
    pub fn running(&self) -> bool {
        self.phase()
            .is_some_and(|phase| matches!(phase, Phase::Trying { running: true, .. }))
    }

    /// Whether it waits on the owner or its own flow, as it is shown.
    #[must_use]
    pub fn open(&self) -> bool {
        !matches!(self, Reading::Unreadable(_))
            && self
                .phase()
                .is_none_or(|phase| matches!(phase, Phase::Trying { .. } | Phase::MayHaveGone))
    }

    /// The try its next tap would be: one more than the try it shows, the first while none.
    #[must_use]
    pub(crate) fn next_attempt(&self) -> u32 {
        match self.phase() {
            Some(Phase::Trying { attempt, .. }) => attempt + 1,
            _ => 1,
        }
    }

    fn phase(&self) -> Option<Phase> {
        self.shown().map(|answer| answer.response.phase())
    }
}

/// `standing`, the response and when, or every candidate of a conflict.
impl std::fmt::Display for Reading {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reading::Standing => f.write_str("standing"),
            Reading::Unreadable(why) => write!(f, "unreadable: {why}"),
            Reading::One(answer) => write!(f, "{answer}"),
            Reading::Conflict(conflict) => {
                f.write_str("conflict[")?;
                for (n, answer) in conflict.iter().enumerate() {
                    if n > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{answer}")?;
                }
                f.write_str("]")
            }
        }
    }
}

/// The response, then when it was recorded.
impl std::fmt::Display for Answer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.response, stamp(self.at))
    }
}

/// What a remember card's words read as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Words {
    /// They are kept: the card shows them.
    Kept(Fact),
    /// They were deleted.
    Gone,
    /// The reading was given no memory to find them in.
    Unread,
}

impl Words {
    /// What a store that was read answers: the words, or that they are gone.
    #[must_use]
    pub fn of(kept: Option<Fact>) -> Self {
        kept.map_or(Words::Gone, Words::Kept)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secretary::responded::Tried;

    fn answer(name: char, reason: &str) -> Answer {
        Answer {
            name: Hash::new(name.to_string().repeat(64)).expect("a name"),
            at: Datetime::new(2026, 10, 19, 9, 0, 0, 0).expect("an instant"),
            response: Responded::Tried {
                attempt: 1,
                tried: Tried::NotSent {
                    reason: reason.to_owned(),
                },
            },
        }
    }

    /// A conflict performs nothing, even where the answer it shows is a try a tap would repeat,
    /// and it lists every side, least named first.
    #[test]
    fn a_conflict_never_performs_and_shows_every_side() {
        let conflict = Reading::Conflict(Conflict {
            first: answer('a', "no network here"),
            second: answer('b', "no network there"),
            rest: vec![answer('c', "no network anywhere")],
        });
        assert!(Reading::One(answer('a', "no network here")).performs());
        assert!(!conflict.performs());
        assert_eq!(conflict.answers().len(), 3);
        assert_eq!(
            conflict.shown().map(|shown| shown.name.as_str().to_owned()),
            Some("a".repeat(64))
        );
        assert!(conflict.to_string().starts_with("conflict[NotSent"));
    }
}
