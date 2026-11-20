//! A git object's name.

use crate::Error;
use std::fmt;

/// A git object's full name: 40 lowercase hexadecimal digits, as `git rev-parse` prints it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Oid(String);

impl Oid {
    pub fn parse(text: &str) -> Result<Oid, Error> {
        if text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            Ok(Oid(text.to_owned()))
        } else {
            Err(Error::Malformed(format!(
                "\"{text}\" is not a full object name"
            )))
        }
    }

    /// Its first twelve digits, as the train's messages print a commit.
    #[must_use]
    pub fn short(&self) -> &str {
        &self.0[..12]
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
