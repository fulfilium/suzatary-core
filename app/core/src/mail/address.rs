//! An email address, parsed once.
//!
//! The check is deliberately thin: it tells an address from a name, and leaves
//! deliverability to the transport, since a stricter parser would refuse
//! addresses that work.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::refusal::Refusal;

/// One address, shaped by its constructor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EmailAddress(String);

impl EmailAddress {
    /// Parse one.
    ///
    /// # Errors
    /// [`Refusal::bad_request`] when `text` is not `local@domain.example`-shaped:
    /// empty on either side of the `@`, no `.` in the domain, or internal
    /// whitespace.
    pub fn new(text: &str) -> Result<Self, Refusal> {
        let trimmed = text.trim();
        let malformed = || Refusal::bad_request(format!("not an email address: {trimmed:?}"));
        let (local, domain) = trimmed.split_once('@').ok_or_else(malformed)?;
        if local.is_empty() || !domain.contains('.') || trimmed.contains(char::is_whitespace) {
            return Err(malformed());
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The address, as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The same address in lower case; lowering a character adds no `@`, `.` or whitespace, so
    /// the result keeps the shape [`EmailAddress::new`] checked.
    #[must_use]
    pub fn lowercased(&self) -> Self {
        Self(self.0.to_lowercase())
    }
}

impl fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for EmailAddress {
    type Error = Refusal;
    fn try_from(text: String) -> Result<Self, Refusal> {
        Self::new(&text)
    }
}

impl From<EmailAddress> for String {
    fn from(address: EmailAddress) -> String {
        address.0
    }
}

// The literal of each is its text, as an inbox act stores it.
prodrome_core::text_literal!(
    EmailAddress => |address| String::from(address.clone()),
);
