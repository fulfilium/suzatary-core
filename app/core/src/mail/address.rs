//! An email address, parsed once: ASCII alone, since a letter from another script, a bidi
//! control or a zero-width character could make one address read as another on a card.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::refusal::Refusal;

/// One address, shaped by its constructor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EmailAddress(String);

/// The characters a local part may hold beside letters and digits (RFC 5322's `atext`).
const LOCAL: &str = "!#$%&'*+-/=?^_`{|}~";

impl EmailAddress {
    /// Parse one: dots between runs of letters, digits and `LOCAL`, one `@`, then two or more
    /// labels of letters, digits and inner hyphens.
    ///
    /// # Errors
    /// [`Refusal::bad_request`] naming the rule `text` breaks.
    pub fn new(text: &str) -> Result<Self, Refusal> {
        let trimmed = text.trim();
        let refused =
            |why: &str| Refusal::bad_request(format!("not an email address: {trimmed:?}: {why}"));
        if !trimmed.is_ascii() {
            return Err(refused(
                "ASCII only; write another script's domain as punycode (xn--)",
            ));
        }
        let (local, domain) = trimmed
            .split_once('@')
            .ok_or_else(|| refused("it has no @"))?;
        if domain.contains('@') {
            return Err(refused("it has more than one @"));
        }
        let runs = |part: &str, allowed: &dyn Fn(char) -> bool| {
            !part.is_empty()
                && part
                    .split('.')
                    .all(|run| !run.is_empty() && run.chars().all(allowed))
        };
        if !runs(local, &|c| c.is_ascii_alphanumeric() || LOCAL.contains(c)) {
            return Err(refused("the part before the @ is not plain"));
        }
        let label = |c: char| c.is_ascii_alphanumeric() || c == '-';
        if domain.len() > 253
            || !domain.contains('.')
            || !runs(domain, &label)
            || domain
                .split('.')
                .any(|part| part.len() > 63 || part.starts_with('-') || part.ends_with('-'))
        {
            return Err(refused("the domain is not two or more plain labels"));
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The address, as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The same address in lower case; lowering an ASCII letter keeps the shape
    /// [`EmailAddress::new`] checked.
    #[must_use]
    pub(crate) fn lowercased(&self) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// THE ATTACK: an address that could read as another on a card (a letter from another
        /// script, a bidi control, a zero-width or soft hyphen, a second `@`, a space) is
        /// refused, wherever in the address it sits; the plain address it imitates is kept.
        #[test]
        fn an_address_that_could_read_as_another_is_refused(
            local in "[a-z]{1,8}(\\.[a-z0-9+_-]{1,5})?",
            domain in "[a-z]{1,8}(-[a-z]{1,4})?\\.example",
            at in 0usize..40,
            posing in prop::sample::select(&[
                "\u{43e}", "\u{430}", "\u{202e}", "\u{200b}", "\u{200d}", "\u{ad}",
                "\u{feff}", "\u{2066}", "@", " ", "\t", "\u{ff20}",
            ][..]),
        ) {
            let plain = format!("{local}@{domain}");
            prop_assert_eq!(EmailAddress::new(&plain).map(String::from), Ok(plain.clone()));
            let at = plain.char_indices().map(|(i, _)| i).nth(at % plain.len()).unwrap_or(0);
            let posed = format!("{}{posing}{}", &plain[..at], &plain[at..]);
            prop_assert!(
                EmailAddress::new(&posed).is_err() || posed.trim() == plain,
                "{posed:?} was admitted"
            );
        }
    }

    #[test]
    fn a_domain_outside_ascii_is_accepted_as_its_punycode_alone() {
        assert!(EmailAddress::new("counsel@xn--lwfirm-4ye.example").is_ok());
        assert!(EmailAddress::new("counsel@l\u{430}wfirm.example").is_err());
        assert!(EmailAddress::new("counsel@-lawfirm.example").is_err());
        assert!(EmailAddress::new("counsel@lawfirm").is_err());
        assert!(EmailAddress::new(".counsel@lawfirm.example").is_err());
        assert!(EmailAddress::new("coun..sel@lawfirm.example").is_err());
    }
}
