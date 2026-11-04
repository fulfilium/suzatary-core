//! A proposal joined with its inbox object, and why one may not be withdrawn.

use super::Asking;
use crate::capability::Datetime;
use crate::secretary::responded::{Done, Responded, Tried, Withdrawal};

/// A proposal and whatever became of it: its line joined with its inbox object
/// ([`Stored::standing`](super::Stored::standing)).
#[derive(Debug, Clone, PartialEq)]
pub struct Standing {
    /// The proposal as its conversation records it.
    pub asking: Asking,
    /// Its inbox object: the act, its reasons and its reading.
    pub object: crate::secretary::inbox::Object,
    /// The proposal it replaces, and how much of that one it carries.
    pub revises: Option<Revised>,
}

/// The proposal a card replaces, as its reader is shown it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Revised {
    /// The replaced proposal's index in the same conversation.
    pub card: usize,
    /// How many of this card's changeset rows, from the first, the replaced card already asked
    /// for: every row after them is new. `0` where either is not a changeset.
    pub carried: usize,
}

/// Why proposal `index`, at `reading`, may not be withdrawn or revised: only one
/// nothing completed may, since a send that may have gone is never replaced by a second draft.
/// `now` is only for a running flow, the one response that goes stale on its own.
#[must_use]
pub fn refused(
    index: usize,
    reading: &crate::secretary::inbox_reading::Reading,
    now: Datetime,
) -> crate::refusal::Refusal {
    let conflict = crate::refusal::Refusal::conflict;
    if let crate::secretary::inbox_reading::Reading::Unreadable(why) = reading {
        return conflict(format!("proposal {index} cannot be read: {why}"));
    }
    let gone = |reason: &str| {
        conflict(format!(
            "proposal {index} may have gone through ({reason}); it is never performed again"
        ))
    };
    match reading.shown().map(|answer| &answer.response) {
        None
        | Some(Responded::Tried {
            tried: Tried::NotSent { .. } | Tried::Lacks(_),
            ..
        }) => conflict(format!(
            "proposal {index} waits on the owner's tap; only they can answer it"
        )),
        Some(Responded::Done(Done::MayHaveGone { reason })) => gone(reason),
        Some(Responded::Checked(check)) => gone(check.reason()),
        Some(Responded::Tried {
            tried: tried @ Tried::Running { began, expires_at },
            ..
        }) => {
            if tried.stale(now) {
                conflict(format!(
                    "proposal {index} expired at {expires_at} before it was used; \
                     a tap asks for a fresh one"
                ))
            } else {
                conflict(format!(
                    "proposal {index} is already running: a device code was issued at \
                     {began}, it expires at {expires_at}"
                ))
            }
        }
        Some(Responded::Done(Done::Sent { .. })) => conflict(format!(
            "proposal {index} has already been sent; an irreversible act is performed once"
        )),
        Some(Responded::Done(Done::Applied { objects, chain })) => conflict(format!(
            "proposal {index} was already applied: {} object(s) are on {chain}",
            objects.len()
        )),
        Some(Responded::Done(Done::Acted { what })) => {
            conflict(format!("proposal {index} was already performed: {what}"))
        }
        Some(Responded::Dismissed { reason }) => {
            conflict(format!("proposal {index} was dismissed ({reason})"))
        }
        Some(Responded::Withdrawn(Withdrawal::Superseded)) => {
            conflict(format!("proposal {index} was replaced by a later one"))
        }
        Some(Responded::Withdrawn(Withdrawal::NoLongerNeeded { why })) => {
            conflict(format!("proposal {index} was withdrawn ({why})"))
        }
    }
}
