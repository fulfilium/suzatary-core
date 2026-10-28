//! What the box has observed of one connection: the facts its steps are met by.

use crate::capability::SecretState;
use crate::health::Health;
use crate::mail::address::EmailAddress;

/// The observations a connection's steps read, each a function the reader lends.
pub struct Seen<'a> {
    /// Where the secret named so stands.
    pub secret: &'a dyn Fn(&str) -> SecretState,
    /// Whether the account holds its sign-in to read mail.
    pub signed_in: &'a dyn Fn(&EmailAddress) -> bool,
    /// Whether the account holds its consent to send mail, a grant apart from reading's.
    pub may_send: &'a dyn Fn(&EmailAddress) -> bool,
    /// Whether its check has ever succeeded.
    pub checked: Checked,
    /// How the person reaches the app, which a terminal's step is spelled in.
    pub reach: &'a super::Reach,
}

/// Whether a connection's check has ever succeeded, which is how the box sees a step it cannot
/// observe itself (a terminal's, the subscription's sign-in) taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checked {
    /// It has not, or nothing has run it.
    Never,
    /// It has, at least once.
    Once,
}

impl Checked {
    /// What `health` says: once its latest success exists.
    #[must_use]
    pub fn of(health: Option<&Health>) -> Self {
        match health.and_then(Health::succeeded) {
            Some(_) => Self::Once,
            None => Self::Never,
        }
    }
}
