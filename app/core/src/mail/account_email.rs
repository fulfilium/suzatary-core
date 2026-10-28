//! An account's canonical address: an [`EmailAddress`] spelled in lower case, so two accounts
//! cannot claim one mailbox by its case, and the address every per-account name is derived from
//! has one spelling.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::mail::account::RosterError;
use crate::mail::address::EmailAddress;

/// An account's address, in lower case.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AccountEmail(EmailAddress);

impl AccountEmail {
    /// Parse one.
    ///
    /// An address with a capital is refused rather than folded: the token files and health
    /// registers are named by the address as written, so folding it here would orphan them.
    ///
    /// # Errors
    /// [`RosterError::Email`] for text that is not an address, and [`RosterError::NotFolded`]
    /// for one that is not in lower case.
    pub fn new(text: &str) -> Result<Self, RosterError> {
        let address = EmailAddress::new(text).map_err(|_| RosterError::Email(text.to_owned()))?;
        let folded = address.lowercased();
        if folded != address {
            return Err(RosterError::NotFolded {
                given: address.into(),
                folded: folded.into(),
            });
        }
        Ok(Self(address))
    }

    /// A new account's address, folded: no name is derived from it yet, so nothing is orphaned.
    #[must_use]
    pub fn folded(address: &EmailAddress) -> Self {
        Self(address.lowercased())
    }

    /// The address, as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// The address.
    #[must_use]
    pub const fn address(&self) -> &EmailAddress {
        &self.0
    }

    /// Whether `address`, in any case, is this account's.
    #[must_use]
    pub fn is(&self, address: &str) -> bool {
        self.as_str().eq_ignore_ascii_case(address.trim())
    }
}

/// Printed as its text, as `doctor` and the instance's golden reading show it.
impl fmt::Debug for AccountEmail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for AccountEmail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<String> for AccountEmail {
    type Error = RosterError;
    fn try_from(text: String) -> Result<Self, RosterError> {
        Self::new(&text)
    }
}

impl From<AccountEmail> for String {
    fn from(email: AccountEmail) -> String {
        email.0.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The instance file's text is parsed by the same door.
    #[test]
    fn the_stored_spelling_parses_through_the_constructor() {
        let parsed: Result<AccountEmail, _> = serde_json::from_str(r#""Ada@example.test""#);
        assert!(parsed.is_err());
        let parsed: AccountEmail = serde_json::from_str(r#""ada@example.test""#).expect("parses");
        assert_eq!(
            serde_json::to_string(&parsed).expect("prints"),
            r#""ada@example.test""#
        );
    }
}
