//! What became of a proposal: each response its state register records, built from what
//! performing it answered ([`Performed`]), a check of a send that may have gone, the owner's no,
//! or a withdrawal by the box or the secretary.

use prodrome_core::literal::{Datetime, Value};

use crate::secretary::phase::Phase;
use prodrome_core::literal::codec::{Constructor, Literal as _, record};

/// A try that did not settle the proposal, so a tap may perform it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tried {
    /// Nothing was sent.
    NotSent {
        /// Why.
        reason: String,
    },
    /// A device-code flow was issued and ends by itself.
    Running {
        /// When the code was issued, as the flow stamped it.
        began: String,
        /// When it goes stale.
        expires_at: String,
    },
}

impl Tried {
    /// Whether it has gone stale by `now`: a flow past its code's expiry. An expiry that does not
    /// parse is never past, so a running act refuses "already running" rather than admit a claim
    /// this box cannot check.
    #[must_use]
    pub fn stale(&self, now: Datetime) -> bool {
        match self {
            Tried::Running { expires_at, .. } => prodrome_core::fpl::parse_iso(expires_at)
                .is_ok_and(|at| prodrome_core::fpl::instant_of(now) > at),
            Tried::NotSent { .. } => false,
        }
    }
}

/// What performing settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    /// The far end took the mail.
    Sent {
        /// Every address the envelope carried.
        recipients: Vec<String>,
        /// The `Message-Id` this box minted.
        message_id: String,
    },
    /// The changeset's acts are on the chain.
    Applied {
        /// Every object appended, in the order the acts were made.
        objects: Vec<String>,
        /// Which chain.
        chain: String,
    },
    /// The box's act ran to completion.
    Acted {
        /// What happened.
        what: String,
    },
    /// It may have happened, so it is never performed again.
    MayHaveGone {
        /// What the performer knew.
        reason: String,
    },
}

/// Why the box or the secretary took a card back before anyone answered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Withdrawal {
    /// A later card of the same conversation replaces it; the conversation's line for that card
    /// names which (`revises`).
    Superseded,
    /// Nothing needs it any more.
    NoLongerNeeded {
        /// Why, in the words of whoever withdrew it.
        why: String,
    },
}

/// What performing a proposal answered: a try, or what it settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Performed {
    /// Tried, and not settled.
    Tried(Tried),
    /// Settled.
    Done(Done),
}

impl From<Tried> for Performed {
    fn from(tried: Tried) -> Self {
        Performed::Tried(tried)
    }
}

impl From<Done> for Performed {
    fn from(done: Done) -> Self {
        Performed::Done(done)
    }
}

impl Performed {
    /// A failed send, by what its transport knew: nothing sent, or a message that may have gone.
    #[must_use]
    pub fn of_send(failed: &crate::mail::send::SendError) -> Self {
        let reason = failed.to_string();
        match failed.delivery() {
            crate::mail::send::Delivery::NotSent => Tried::NotSent { reason }.into(),
            crate::mail::send::Delivery::Unknown => Done::MayHaveGone { reason }.into(),
        }
    }
}

/// What a check prints: its signature, after an event's head, and its values.
pub trait CheckFields {
    /// Its signature and values, in print order.
    fn body(&self) -> (Constructor, Vec<Value>);
}

/// A check as written holds nothing.
impl CheckFields for () {
    fn body(&self) -> (Constructor, Vec<Value>) {
        (CHECKED, Vec::new())
    }
}

/// `NotSent(attempt, reason)`, after an event's head.
pub const NOT_SENT: Constructor = ("NotSent", &["attempt", "reason"]);
/// `Started(attempt, began, expires_at)`.
pub const STARTED: Constructor = ("Started", &["attempt", "began", "expires_at"]);
/// `Sent(recipients, message_id)`.
pub const SENT: Constructor = ("Sent", &["recipients", "message_id"]);
/// `Applied(objects, chain)`.
pub const APPLIED: Constructor = ("Applied", &["objects", "chain"]);
/// `Acted(what)`.
pub const ACTED: Constructor = ("Acted", &["what"]);
/// `MayHaveGone(reason)`.
pub const MAY_HAVE_GONE: Constructor = ("MayHaveGone", &["reason"]);
/// `Checked()`, as written.
pub const CHECKED: Constructor = ("Checked", &[]);
/// `Dismissed(reason)`.
pub const DISMISSED: Constructor = ("Dismissed", &["reason"]);
/// `Superseded()`.
pub const SUPERSEDED: Constructor = ("Superseded", &[]);
/// `NoLongerNeeded(why)`.
pub const NO_LONGER_NEEDED: Constructor = ("NoLongerNeeded", &["why"]);

/// One response. A check holds `C`: nothing as it is written, and the send it checks once a
/// reading has found that send beneath it ([`crate::secretary::inbox::Check`]), so a check of nothing has no reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Responded<C = ()> {
    /// The `attempt`-th try, which did not settle it.
    Tried {
        /// One for the first try, one more for each after it.
        attempt: u32,
        /// What it came to.
        tried: Tried,
    },
    /// Performing settled it.
    Done(Done),
    /// The owner checked a send that may have gone.
    Checked(C),
    /// The owner said no.
    Dismissed {
        /// His words, verbatim.
        reason: String,
    },
    /// The box or the secretary took it back before anyone answered it.
    Withdrawn(Withdrawal),
}

impl Responded {
    /// The response performing answered, as the `attempt`-th try where it is one.
    #[must_use]
    pub fn performed(performed: Performed, attempt: u32) -> Self {
        match performed {
            Performed::Tried(tried) => Responded::Tried { attempt, tried },
            Performed::Done(done) => Responded::Done(done),
        }
    }
}

impl<C> Responded<C> {
    /// Its point of the machine.
    #[must_use]
    pub fn phase(&self) -> Phase {
        match self {
            Responded::Tried { attempt, tried } => Phase::Trying {
                attempt: *attempt,
                running: matches!(tried, Tried::Running { .. }),
            },
            Responded::Done(Done::Sent { .. }) => Phase::Sent,
            Responded::Done(Done::Applied { .. }) => Phase::Applied,
            Responded::Done(Done::Acted { .. }) => Phase::Acted,
            Responded::Done(Done::MayHaveGone { .. }) => Phase::MayHaveGone,
            Responded::Checked(_) => Phase::Checked,
            Responded::Dismissed { .. } => Phase::Dismissed,
            Responded::Withdrawn(_) => Phase::Withdrawn,
        }
    }

    /// Whether a tap performs it again: a try that sent nothing.
    #[must_use]
    pub fn performs(&self) -> bool {
        matches!(
            self,
            Responded::Tried {
                tried: Tried::NotSent { .. },
                ..
            }
        )
    }
}

impl<C: CheckFields> Responded<C> {
    /// Its signature, after an event's head, and its values, in print order.
    #[must_use]
    pub fn body(&self) -> (Constructor, Vec<Value>) {
        match self {
            Responded::Tried {
                attempt,
                tried: Tried::NotSent { reason },
            } => (NOT_SENT, vec![attempt.to_value(), reason.to_value()]),
            Responded::Tried {
                attempt,
                tried: Tried::Running { began, expires_at },
            } => (
                STARTED,
                vec![attempt.to_value(), began.to_value(), expires_at.to_value()],
            ),
            Responded::Done(Done::Sent {
                recipients,
                message_id,
            }) => (SENT, vec![recipients.to_value(), message_id.to_value()]),
            Responded::Done(Done::Applied { objects, chain }) => {
                (APPLIED, vec![objects.to_value(), chain.to_value()])
            }
            Responded::Done(Done::Acted { what }) => (ACTED, vec![what.to_value()]),
            Responded::Done(Done::MayHaveGone { reason }) => {
                (MAY_HAVE_GONE, vec![reason.to_value()])
            }
            Responded::Dismissed { reason } => (DISMISSED, vec![reason.to_value()]),
            Responded::Withdrawn(Withdrawal::Superseded) => (SUPERSEDED, Vec::new()),
            Responded::Withdrawn(Withdrawal::NoLongerNeeded { why }) => {
                (NO_LONGER_NEEDED, vec![why.to_value()])
            }
            Responded::Checked(check) => check.body(),
        }
    }
}

/// Its literal: the constructor and its fields, as an event prints them after its head.
impl<C: CheckFields> std::fmt::Display for Responded<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (signature, values) = self.body();
        f.write_str(&prodrome_core::literal::print_literal(&record(
            signature, values,
        )))
    }
}
