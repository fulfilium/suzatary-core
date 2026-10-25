//! Why a connection's check failed, as a class the box can act on, and the one fix each class
//! has.
//!
//! A class is parsed once, where the failure is observed (a fetch's status, the host's refusal,
//! a sync's sign-in, a model's refused credential), and stored with it; nothing reads a failure's
//! words to learn its class again.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::plugin::Refusal;

/// What a failed check came to, as the box classifies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "FailureClass")]
pub enum Failure {
    /// The service refused the link it was given: it was rotated or revoked.
    Revoked {
        /// What the service answered.
        status: Revocation,
    },
    /// The account's sign-in is missing or lapsed.
    NeedsLogin,
    /// A secret it reads is stored nowhere on this box.
    Unresolved,
    /// The model refused its key or token.
    KeyRefused,
    /// The value stored for its link is not one the box sends: not a URL it reads, or one in
    /// the clear. Nothing was sent, so no service answered.
    Unusable,
    /// Its address could not be reached: the request never left, no answer came before the
    /// deadline, or the answer was lost on the way.
    Unreachable,
    /// Anything else: an outage, a timeout, an answer the box could not store.
    Unclassified,
}

/// The statuses a service refuses a link with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Revocation {
    /// 401.
    Unauthorized,
    /// 403.
    Forbidden,
    /// 404.
    NotFound,
}

impl Revocation {
    /// The status, as the service sent it.
    #[must_use]
    pub const fn status(self) -> u16 {
        match self {
            Revocation::Unauthorized => 401,
            Revocation::Forbidden => 403,
            Revocation::NotFound => 404,
        }
    }
}

/// What the owner can do about a failure: exactly one per class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remedy {
    /// Store a secret again, from a Secret card.
    Secret,
    /// Sign the account in again, from an Auth card.
    Auth,
    /// Nothing a tap mends: the next run tries again. A mail account failing for a day still
    /// proposes its todo (`secretary::mail_health`), and one failing for a week is offered for
    /// removal.
    Retry,
}

impl Failure {
    /// The class of a fetch answered with `status` that was not stored.
    #[must_use]
    pub const fn of_status(status: u16) -> Self {
        match status {
            401 => Failure::Revoked {
                status: Revocation::Unauthorized,
            },
            403 => Failure::Revoked {
                status: Revocation::Forbidden,
            },
            404 => Failure::Revoked {
                status: Revocation::NotFound,
            },
            _ => Failure::Unclassified,
        }
    }

    /// The class of a fetch the host refused to send or to hand back.
    #[must_use]
    pub const fn of_refusal(refusal: &Refusal) -> Self {
        match refusal {
            Refusal::CredentialUnset(_) => Failure::Unresolved,
            Refusal::CredentialNotAUrl(_) | Refusal::InTheClear(_) => Failure::Unusable,
            Refusal::BadUrl(_)
            | Refusal::OriginNotAllowed(_)
            | Refusal::CredentialNotDeclared(_)
            | Refusal::CredentialUnavailable(_)
            | Refusal::Echoed(_)
            | Refusal::TooManyFetches => Failure::Unclassified,
            Refusal::Unreachable | Refusal::TimedOut | Refusal::Lost => Failure::Unreachable,
        }
    }

    /// Its one fix.
    #[must_use]
    pub const fn remedy(self) -> Remedy {
        match self {
            Failure::Revoked { .. }
            | Failure::Unresolved
            | Failure::KeyRefused
            | Failure::Unusable => Remedy::Secret,
            Failure::NeedsLogin => Remedy::Auth,
            Failure::Unreachable | Failure::Unclassified => Remedy::Retry,
        }
    }

    /// Every class, each revocation once: what a law ranges over.
    pub const ALL: [Failure; 9] = [
        Failure::Revoked {
            status: Revocation::Unauthorized,
        },
        Failure::Revoked {
            status: Revocation::Forbidden,
        },
        Failure::Revoked {
            status: Revocation::NotFound,
        },
        Failure::NeedsLogin,
        Failure::Unresolved,
        Failure::KeyRefused,
        Failure::Unusable,
        Failure::Unreachable,
        Failure::Unclassified,
    ];
}
