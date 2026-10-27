//! A value's name by its content: the SHA-256 of its canonical literal, so two values are one
//! name exactly when they print alike, and a name says which type it names.

use std::marker::PhantomData;

use sha2::{Digest, Sha256};

use prodrome_core::literal::codec::Literal;

/// The name of a `T` by its content.
#[derive(Debug)]
pub struct ContentId<T> {
    hex: String,
    of: PhantomData<fn() -> T>,
}

impl<T: Literal> ContentId<T> {
    /// The name of `value`: the SHA-256, in lowercase hex, of its literal's canonical print.
    #[must_use]
    pub fn of(value: &T) -> Self {
        let print = prodrome_core::literal::print_literal(&value.to_value());
        Self {
            hex: hex::encode(Sha256::digest(print.as_bytes())),
            of: PhantomData,
        }
    }
}

impl<T> ContentId<T> {
    /// The 64 hex digits.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.hex
    }
}

impl<T> Clone for ContentId<T> {
    fn clone(&self) -> Self {
        Self {
            hex: self.hex.clone(),
            of: PhantomData,
        }
    }
}

impl<T> PartialEq for ContentId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.hex == other.hex
    }
}

impl<T> Eq for ContentId<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two values are one name exactly when their literals print alike.
    #[test]
    fn a_name_is_its_print() {
        let a = ContentId::of(&"a".to_owned());
        assert_eq!(a, ContentId::of(&"a".to_owned()));
        assert_ne!(a, ContentId::of(&"b".to_owned()));
        assert_eq!(
            a.as_str(),
            hex::encode(Sha256::digest(b"'a'")),
            "the print of a string is its quoted literal"
        );
    }
}
