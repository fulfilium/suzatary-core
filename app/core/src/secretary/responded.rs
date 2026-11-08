//! The responses a proposal's state register records. The constructors below are a stored format.

use prodrome_core::literal::{Datetime, Value};

use crate::secretary::phase::Phase;
use prodrome_core::literal::codec::{Constructor, Literal as _, record};

/// A try that did not settle the proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tried {
    /// Nothing was sent.
    NotSent {
        /// Why.
        reason: String,
    },
    /// The box lacks a grant the act needs.
    Lacks(crate::mail::lacking::Lacking),
    /// A tap again would meet the same refusal, so none is offered.
    Refused {
        /// Why.
        reason: String,
    },
    /// Part of the act left the box; a tap again finishes it without repeating that part.
    Unfinished {
        /// What happened.
        reason: String,
        /// An https page the box builds, never a model's.
        link: String,
    },
    /// A device-code flow was issued and ends by itself.
    Running {
        /// As the flow stamped it.
        began: String,
        /// When it goes stale.
        expires_at: String,
    },
}

impl Tried {
    /// A flow past its code's expiry. An unparsable expiry is never past, so the box refuses
    /// "already running" rather than admit a claim it cannot check.
    #[must_use]
    pub fn stale(&self, now: Datetime) -> bool {
        match self {
            Tried::Running { expires_at, .. } => prodrome_core::fpl::parse_iso(expires_at)
                .is_ok_and(|at| prodrome_core::fpl::instant_of(now) > at),
            Tried::NotSent { .. }
            | Tried::Lacks(_)
            | Tried::Refused { .. }
            | Tried::Unfinished { .. } => false,
        }
    }
}

/// What performing settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    /// The far end took the mail.
    Sent {
        /// The envelope's addresses.
        recipients: Vec<String>,
        /// The `Message-Id` this box minted.
        message_id: String,
    },
    /// The changeset's acts are on the chain.
    Applied {
        /// In the order the acts were made.
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

/// Why a card was taken back before anyone answered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Withdrawal {
    /// A later card replaces it; that card's line names which (`revises`).
    Superseded,
    /// Nothing needs it any more.
    NoLongerNeeded {
        /// In the withdrawer's words.
        why: String,
    },
    /// The box saw it done another way.
    DoneElsewhere {
        /// What the box saw.
        what: String,
    },
}

/// What performing a proposal answered.
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
    /// A failed send, by what its transport knew.
    #[must_use]
    pub fn of_send(failed: &crate::mail::send::SendError) -> Self {
        if let crate::mail::send::SendError::Ungranted(lacking) = failed {
            return Tried::Lacks(lacking.clone()).into();
        }
        let reason = failed.to_string();
        match failed.delivery() {
            crate::mail::send::Delivery::NotSent => Tried::NotSent { reason }.into(),
            crate::mail::send::Delivery::Unknown => Done::MayHaveGone { reason }.into(),
        }
    }
}

/// What a check prints after an event's head.
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

/// Stored after an event's head.
pub(crate) const NOT_SENT: Constructor = ("NotSent", &["attempt", "reason"]);
/// Stored.
pub(crate) const LACKS: Constructor = ("Lacks", &["attempt", "lacking"]);
/// Stored.
pub(crate) const REFUSED: Constructor = ("Refused", &["attempt", "reason"]);
/// Stored.
pub(crate) const UNFINISHED: Constructor = ("Unfinished", &["attempt", "reason", "link"]);
/// Stored for [`Tried::Running`].
pub(crate) const STARTED: Constructor = ("Started", &["attempt", "began", "expires_at"]);
/// Stored.
pub const SENT: Constructor = ("Sent", &["recipients", "message_id"]);
/// Stored.
pub(crate) const APPLIED: Constructor = ("Applied", &["objects", "chain"]);
/// Stored.
pub(crate) const ACTED: Constructor = ("Acted", &["what"]);
/// Stored.
pub(crate) const MAY_HAVE_GONE: Constructor = ("MayHaveGone", &["reason"]);
/// Stored, as written.
pub(crate) const CHECKED: Constructor = ("Checked", &[]);
/// Stored.
pub(crate) const DISMISSED: Constructor = ("Dismissed", &["reason"]);
/// Stored.
pub(crate) const SUPERSEDED: Constructor = ("Superseded", &[]);
/// Stored.
pub(crate) const NO_LONGER_NEEDED: Constructor = ("NoLongerNeeded", &["why"]);
/// Stored.
pub(crate) const DONE_ELSEWHERE: Constructor = ("DoneElsewhere", &["what"]);

/// One response. A check holds `C`: nothing as written, and the send it checks once a reading
/// finds it ([`crate::secretary::inbox::Check`]), so a check of nothing has no reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Responded<C = ()> {
    /// A try that did not settle it.
    Tried {
        /// One for the first try.
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
    /// Taken back before anyone answered it.
    Withdrawn(Withdrawal),
}

impl Responded {
    /// The response performing answered.
    #[must_use]
    pub fn performed(performed: Performed, attempt: u32) -> Self {
        match performed {
            Performed::Tried(tried) => Responded::Tried { attempt, tried },
            Performed::Done(done) => Responded::Done(done),
        }
    }
}

impl<C> Responded<C> {
    /// Its phase.
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

    /// The grant a try lacked.
    #[must_use]
    pub fn lacking(&self) -> Option<&crate::mail::lacking::Lacking> {
        match self {
            Responded::Tried {
                tried: Tried::Lacks(lacking),
                ..
            } => Some(lacking),
            Responded::Tried {
                tried:
                    Tried::NotSent { .. }
                    | Tried::Running { .. }
                    | Tried::Refused { .. }
                    | Tried::Unfinished { .. },
                ..
            }
            | Responded::Done(_)
            | Responded::Checked(_)
            | Responded::Dismissed { .. }
            | Responded::Withdrawn(_) => None,
        }
    }

    /// The link of a try that stopped partway.
    #[must_use]
    pub fn unfinished(&self) -> Option<&str> {
        match self {
            Responded::Tried {
                tried: Tried::Unfinished { link, .. },
                ..
            } => Some(link),
            Responded::Tried {
                tried:
                    Tried::NotSent { .. }
                    | Tried::Lacks(_)
                    | Tried::Running { .. }
                    | Tried::Refused { .. },
                ..
            }
            | Responded::Done(_)
            | Responded::Checked(_)
            | Responded::Dismissed { .. }
            | Responded::Withdrawn(_) => None,
        }
    }

    /// A try that sent nothing, which a tap performs again.
    #[must_use]
    pub fn performs(&self) -> bool {
        matches!(
            self,
            Responded::Tried {
                tried: Tried::NotSent { .. }
                    | Tried::Lacks(_)
                    | Tried::Refused { .. }
                    | Tried::Unfinished { .. },
                ..
            }
        )
    }
}

impl<C: CheckFields> Responded<C> {
    /// Its signature and values, in print order.
    #[must_use]
    pub fn body(&self) -> (Constructor, Vec<Value>) {
        match self {
            Responded::Tried {
                attempt,
                tried: Tried::NotSent { reason },
            } => (NOT_SENT, vec![attempt.to_value(), reason.to_value()]),
            Responded::Tried {
                attempt,
                tried: Tried::Lacks(lacking),
            } => (LACKS, vec![attempt.to_value(), lacking.to_value()]),
            Responded::Tried {
                attempt,
                tried: Tried::Refused { reason },
            } => (REFUSED, vec![attempt.to_value(), reason.to_value()]),
            Responded::Tried {
                attempt,
                tried: Tried::Unfinished { reason, link },
            } => (
                UNFINISHED,
                vec![attempt.to_value(), reason.to_value(), link.to_value()],
            ),
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
            Responded::Withdrawn(Withdrawal::DoneElsewhere { what }) => {
                (DONE_ELSEWHERE, vec![what.to_value()])
            }
            Responded::Checked(check) => check.body(),
        }
    }
}

/// As an event prints it after its head.
impl<C: CheckFields> std::fmt::Display for Responded<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (signature, values) = self.body();
        f.write_str(&prodrome_core::literal::print_literal(&record(
            signature, values,
        )))
    }
}
