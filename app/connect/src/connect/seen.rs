//! What the box has observed of one connection: the facts its steps are met by.

use crate::secrets::SecretState;
use suzatary_health::health::Health;
use suzatary_mail::mail::address::EmailAddress;

/// The observations a connection's steps read.
pub struct Seen<'a> {
    pub secret: &'a dyn Fn(&str) -> SecretState,
    /// Whether the account holds its sign-in to read mail.
    pub signed_in: &'a dyn Fn(&EmailAddress) -> bool,
    /// Whether the account holds its consent to send mail, a grant separate from reading's.
    pub may_send: &'a dyn Fn(&EmailAddress) -> bool,
    pub checked: Checked,
    pub reach: &'a super::Reach,
}

/// Whether a connection's check has ever succeeded; the box reads a step it cannot observe (a
/// terminal's, the subscription's sign-in) as taken from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checked {
    Never,
    Once,
}

impl Checked {
    #[must_use]
    pub fn of(health: Option<&Health>) -> Self {
        match health.and_then(Health::succeeded) {
            Some(_) => Self::Once,
            None => Self::Never,
        }
    }
}
