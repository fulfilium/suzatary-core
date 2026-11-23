//! One attendee's name as a calendar keeps it.

use crate::holds_address;

/// The most characters of one attendee's name.
pub const MAX_NAME: usize = 200;

/// Trimmed text that is not empty, at most [`MAX_NAME`] characters, holding nothing the application
/// reads as an email address: an attendee's name and never a way to reach them. The recogniser
/// reads an ASCII address with a dotted domain, so an address written another way passes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttendeeName(String);

/// Why a text is not an attendee's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NotAName {
    #[error("an attendee's name is empty")]
    Empty,
    #[error("an attendee's name holds an email address")]
    Address,
    #[error("an attendee's name is longer than {MAX_NAME} characters")]
    TooLong,
}

impl AttendeeName {
    pub fn parse(text: &str) -> Result<Self, NotAName> {
        let text = text.trim();
        if text.is_empty() {
            return Err(NotAName::Empty);
        }
        if holds_address(text) {
            return Err(NotAName::Address);
        }
        if text.chars().count() > MAX_NAME {
            return Err(NotAName::TooLong);
        }
        Ok(Self(text.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_trimmed_and_bounded_in_characters() {
        let parsed = |text: &str| AttendeeName::parse(text).map(|name| name.as_str().len());
        assert_eq!(
            AttendeeName::parse("  Ada ").map(|name| name.as_str().to_owned()),
            Ok("Ada".to_owned())
        );
        assert_eq!(parsed(&"a".repeat(MAX_NAME)), Ok(MAX_NAME));
        assert_eq!(parsed(&format!("{} ", "a".repeat(MAX_NAME))), Ok(MAX_NAME));
        assert_eq!(parsed(&"a".repeat(MAX_NAME + 1)), Err(NotAName::TooLong));
        assert_eq!(parsed(&"é".repeat(MAX_NAME)), Ok(2 * MAX_NAME));
        assert_eq!(parsed(&"é".repeat(MAX_NAME + 1)), Err(NotAName::TooLong));
        assert_eq!(parsed(" \t "), Err(NotAName::Empty));
        let address = format!("{}@{}", "mira", "example.invalid");
        assert_eq!(parsed(&format!("Mira <{address}>")), Err(NotAName::Address));
    }
}
