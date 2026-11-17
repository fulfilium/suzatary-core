//! Why the host did not send a plugin's request, or did not hand back its answer.

use super::{CredentialName, Origin, Said};

/// A refusal names an origin or a credential's NAME, never a credential's value or a secret
/// URL's path; whatever it repeats of the plugin's own text is [`Said`], so bounded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// Not a URL the host's grammar reads.
    #[error("not a URL this host reads: {0:?}")]
    BadUrl(Said),
    /// An origin the manifest does not allow.
    #[error("{0} is not an origin this plugin may reach")]
    OriginNotAllowed(Origin),
    /// A name the manifest does not declare.
    #[error("{0:?} is not a credential this plugin declares")]
    CredentialNotDeclared(Said),
    /// Declared, and not set on this box.
    #[error("{0} is declared and not set")]
    CredentialUnset(CredentialName),
    /// Declared, and the secret store could not say what it holds (locked, gone or broken).
    #[error("the secret store could not answer for {0}")]
    CredentialUnavailable(CredentialName),
    /// A credential whose value is not a URL the host reads.
    #[error("the value of {0} is not a URL this host reads")]
    CredentialNotAUrl(CredentialName),
    /// A credential whose value is neither https nor loopback: it would travel in the clear.
    #[error("the value of {0} is not an https or loopback URL, and is never sent in the clear")]
    InTheClear(CredentialName),
    /// The answer contained a credential's value, and is withheld.
    #[error("the answer contained the value of {0}, and is withheld")]
    Echoed(CredentialName),
    /// Past the most fetches one call may make.
    #[error("past the {} fetches one call may make", super::MAX_FETCHES)]
    TooManyFetches,
    /// The request never left.
    #[error("unreachable")]
    Unreachable,
    /// No answer before the call's deadline.
    #[error("timed out")]
    TimedOut,
    /// The answer was lost, or larger than the host reads.
    #[error("lost")]
    Lost,
}
