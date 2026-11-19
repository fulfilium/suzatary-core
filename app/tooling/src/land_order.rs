//! A reading as a land weighs it.

use crate::{Outcome, Reading};

/// The chain a land reads, green < unjudged < red < finding. A train's verdict is the greatest of
/// its halves' readings in it, so one half's green never hides the other half's silence.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum LandOrder {
    Green,
    Unjudged,
    Red,
    Finding,
}

impl LandOrder {
    #[must_use]
    pub fn of(reading: Reading) -> LandOrder {
        match reading {
            Reading::Agreed(Outcome::Green) => LandOrder::Green,
            Reading::Unjudged => LandOrder::Unjudged,
            Reading::Agreed(Outcome::Red) => LandOrder::Red,
            Reading::Finding => LandOrder::Finding,
        }
    }

    /// The verdict on a train: the reading of its two halves that weighs more.
    #[must_use]
    pub fn verdict([tree, live]: [Reading; 2]) -> Reading {
        if LandOrder::of(live) > LandOrder::of(tree) {
            live
        } else {
            tree
        }
    }
}
