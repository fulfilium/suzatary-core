//! Where a proposal's machine stands, as its state register orders it: further along is greater.
//!
//! Standing is the register unwritten, below every write. Each try that did not settle it is an
//! [`Phase::Trying`] numbered by its attempt, so a retry is a step up even where the code goes
//! from a running flow back to "not sent" and from "not sent" to a running flow again; without the
//! number those two would sit above each other. Every try is below every settled phase, which is
//! what retrying and then settling needs, and a send that may have gone is below its check. A
//! withdrawal by the box or the secretary is below the owner's no, so the two closings written
//! apart settle as his no, with his words. The settled phases are otherwise pairwise
//! incomparable, so two devices that settled one proposal two ways read as a conflict and never as
//! a winner.
//!
//! A check is above every try too, as transitivity demands, yet it stands only on the send it
//! checks: that is its [`Phase::floor`], which the order cannot say and the append asks.

use prodrome_core::fold::{Inflationary, Order};
use serde::Serialize;
use ts_rs::TS;

/// One point of the machine; on the wire, the state register's value as the browser's module
/// answers it, tagged on `phase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum Phase {
    /// Tried and not settled: the `attempt`-th try, a flow `running` or nothing sent.
    Trying {
        /// One for the first try, one more for each after it.
        attempt: u32,
        /// A device-code flow was issued and runs; otherwise nothing was sent.
        running: bool,
    },
    /// The far end took the mail.
    Sent,
    /// The changeset's acts are on the chain.
    Applied,
    /// The box's act ran to completion.
    Acted,
    /// It may have happened; never performed again.
    MayHaveGone,
    /// The owner checked a send that may have gone.
    Checked,
    /// The owner said no.
    Dismissed,
    /// The box or the secretary took it back before anyone answered it: below the owner's no.
    Withdrawn,
}

impl Phase {
    /// Whether it settled the proposal: every phase but a try.
    #[must_use]
    pub fn settled(self) -> bool {
        !matches!(self, Phase::Trying { .. })
    }

    /// What every response it supersedes must stand at or above, and some response must: a check
    /// stands on the send that may have gone it checks. `None` for a phase any reading admits.
    #[must_use]
    pub fn floor(self) -> Option<Phase> {
        matches!(self, Phase::Checked).then_some(Phase::MayHaveGone)
    }

    /// Whether it may be written over `reading`, the phases it supersedes: on its floor where it
    /// has one. The order is asked beside it, by the append's own `grows`.
    #[must_use]
    pub fn stands_on(self, reading: &[Phase]) -> bool {
        self.floor()
            .is_none_or(|floor| !reading.is_empty() && reading.iter().all(|held| floor.le(held)))
    }
}

impl Order for Phase {
    fn le(&self, other: &Self) -> bool {
        use Phase::{Checked, Dismissed, MayHaveGone, Trying, Withdrawn};
        match (*self, *other) {
            (
                Trying {
                    attempt: a,
                    running: r,
                },
                Trying {
                    attempt: b,
                    running: s,
                },
            ) => a < b || (a == b && r == s),
            (Trying { .. }, _) | (MayHaveGone, Checked) | (Withdrawn, Dismissed) => true,
            (one, two) => one == two,
        }
    }
}

/// The append refuses a response below, or beside, the phase it supersedes.
impl Inflationary for Phase {}

#[cfg(test)]
mod tests {
    use super::*;

    fn trying(attempt: u32, running: bool) -> Phase {
        Phase::Trying { attempt, running }
    }

    #[test]
    fn a_retry_climbs_whichever_way_it_turns() {
        assert!(trying(1, true).le(&trying(2, false)));
        assert!(trying(1, false).le(&trying(2, true)));
        assert!(!trying(1, false).le(&trying(1, true)) && !trying(1, true).le(&trying(1, false)));
        assert!(trying(7, true).le(&Phase::Acted));
    }

    #[test]
    fn sent_and_dismissed_are_the_conflict_they_are() {
        assert!(!Phase::Sent.le(&Phase::Dismissed) && !Phase::Dismissed.le(&Phase::Sent));
        assert!(Phase::Withdrawn.le(&Phase::Dismissed) && !Phase::Dismissed.le(&Phase::Withdrawn));
        assert!(!Phase::Withdrawn.le(&Phase::Sent) && !Phase::Sent.le(&Phase::Withdrawn));
        assert!(trying(3, false).le(&Phase::Withdrawn) && Phase::Withdrawn.settled());
        assert!(Phase::MayHaveGone.le(&Phase::Checked) && !Phase::Checked.le(&Phase::MayHaveGone));
        assert!(Phase::Checked.settled() && !trying(1, false).settled());
    }

    #[test]
    fn only_a_check_has_a_floor() {
        assert_eq!(Phase::Checked.floor(), Some(Phase::MayHaveGone));
        assert!(
            [
                trying(1, true),
                Phase::Sent,
                Phase::MayHaveGone,
                Phase::Dismissed
            ]
            .iter()
            .all(|phase| phase.floor().is_none())
        );
    }
}
